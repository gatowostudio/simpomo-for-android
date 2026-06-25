//! ポモドーロタイマーのコアロジック（状態機械）。
//!
//! 設計方針（ADR-0002）: タイマーの「真実」は Rust が持つ。ここは時計に一切依存しない
//! 純粋な状態機械として実装し、`tick(elapsed)` で時間を進める。本モジュールは単体テストできる。
//!
//! 駆動の契約（重要）: `tick` を呼ぶのは **Rust 側のランタイムタスク**（#3）であって、
//! フロント（WebView）ではない。フロントはイベント受信と `invoke` のみを行う薄い表示層。
//! ランタイムは「前回 tick からの実経過秒」を**単調時計（`std::time::Instant` の差分）**で
//! 算出して渡すこと。固定の `tick(1)` をインターバルで呼ぶ素朴実装はドリフトし、スリープ
//! 復帰時に実時間とずれる。`tick` が複数フェーズ境界を一度に処理できる（スリープ復帰で
//! まとめて経過秒を渡せる）のはこのため。
//! ※ 状態機械自体は tick(経過秒) モデルで時計非依存に保つ。一方 Android ではプロセスが
//!   バックグラウンドで kill/Doze されうるため、上位レイヤ（`session.rs` / `lib.rs`）が
//!   実行状態と**壁時計の anchor** を永続化し、復帰時に「now - anchor」の経過秒を 1 回の
//!   `tick` として与えて取り戻す（deadline 永続化方式、ADR-0002 §3 / option B）。
//!   復元の入口として `state()` / `restore()` を提供する。
//!
//! サイクル数（spec）: `0`=1 セット（作業→休憩）で停止し手動 start 待ち / 有限 `N`=N セット
//! 自動連続で停止 / 無限=停止せず継続。

use serde::{Deserialize, Serialize};

/// 既定の作業時間（25 分）。
pub const DEFAULT_WORK_SECS: u32 = 25 * 60;
/// 既定の休憩時間（5 分）。
pub const DEFAULT_BREAK_SECS: u32 = 5 * 60;
/// 1 フェーズの最小秒数。0 秒フェーズは tick の無限ループを招くため下限を設ける。
pub const MIN_PHASE_SECS: u32 = 1;

/// 現在のフェーズ（作業 / 休憩）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    Work,
    Break,
}

/// タイマーの稼働状態。
///
/// `Idle` は「未開始 / 停止中（start 待ち）」を表す。セッション完了後もここへ戻り、
/// 次の start で新しいセッションが始まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Idle,
    Running,
    Paused,
}

/// 自動継続するサイクル（セット）数の設定。
///
/// - `Finite(0)`: 既定。1 セットだけ実行して停止（手動 start 待ち）。
/// - `Finite(n)` (n>=1): n セットを自動連続実行して停止。
/// - `Infinite`: 停止せず自動継続。
///
/// 注: 仕様上 `Finite(0)` と `Finite(1)` は「1 セットで停止」という同一挙動になる。
/// `0` は「自動継続なし」を表す既定値としての綴りであり、意図的な等価である。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleSetting {
    Finite(u32),
    Infinite,
}

impl CycleSetting {
    /// 実際に走らせるセット数。`Infinite` は上限なしを表す `None`。
    ///
    /// 注: ここで `Finite(0)` を `Some(1)` に畳むのは「走らせる回数」を求めるときだけ。
    /// 設定値そのもの（既定の `0` を含む）は `Config` に保持され、#5 の永続化/設定 UI とは
    /// その生値で往復する（`0` が `1` に化けない）。
    fn target_sets(self) -> Option<u32> {
        match self {
            CycleSetting::Finite(0) => Some(1),
            CycleSetting::Finite(n) => Some(n),
            CycleSetting::Infinite => None,
        }
    }

    /// `completed` セットを完了した時点でセッションを終えるべきか。無限設定では終わらない。
    fn is_last_set(self, completed: u32) -> bool {
        match self.target_sets() {
            Some(target) => completed >= target,
            None => false,
        }
    }
}

/// タイマーの設定値。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    pub work_secs: u32,
    pub break_secs: u32,
    pub cycles: CycleSetting,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            work_secs: DEFAULT_WORK_SECS,
            break_secs: DEFAULT_BREAK_SECS,
            cycles: CycleSetting::Finite(0),
        }
    }
}

impl Config {
    /// 不正値（0 秒フェーズ）を最小値へ丸めた健全な設定を返す。
    fn sanitized(self) -> Self {
        Self {
            work_secs: self.work_secs.max(MIN_PHASE_SECS),
            break_secs: self.break_secs.max(MIN_PHASE_SECS),
            cycles: self.cycles,
        }
    }
}

/// フェーズ境界で発生する出来事。呼び出し側が通知音（#6）などに使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TimerEvent {
    /// 作業フェーズが終了した（→ 休憩へ）。タイマー終了音の契機。
    WorkEnded,
    /// 休憩フェーズが終了した。休憩終了音の契機。
    BreakEnded,
    /// 設定されたセット数を完了し、セッションが停止した（Idle に戻った）。
    SessionFinished,
}

/// フロントへ渡す表示用スナップショット。
///
/// `camelCase` のフィールド名はフロント（`src/lib/timer.ts` の手書きミラー）との契約。
/// フィールド名や rename を変えたら timer.ts も必ず同期すること
/// （`snapshot_serializes_to_camel_case_keys` テストがキー名のドリフトを検出する）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerSnapshot {
    pub phase: Phase,
    pub status: Status,
    pub remaining_secs: u32,
    /// 現在のセット番号（0 始まり）。表示時は +1 する想定。
    pub set_index: u32,
    /// 走らせるセット総数。無限のときは `None`。
    pub total_sets: Option<u32>,
    pub work_secs: u32,
    pub break_secs: u32,
}

/// 永続化・復元するタイマーの実行状態（ADR-0002 §3）。`config` は含めない
/// （設定の真実は settings.json で、復元時に別途与える）。`session.json` に保存される内部状態で、
/// フロントとの契約ではないため命名は内部都合（camelCase）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerState {
    pub phase: Phase,
    pub status: Status,
    pub remaining_secs: u32,
    pub set_index: u32,
}

/// ポモドーロタイマーの状態機械。
#[derive(Debug, Clone, Copy)]
pub struct Timer {
    config: Config,
    phase: Phase,
    status: Status,
    remaining: u32,
    /// 現在のセット番号（0 始まり）。「これまで完了したセット数」は `set_index + 1` で表す。
    set_index: u32,
}

impl Timer {
    /// 設定からタイマーを生成する。初期状態は「作業フェーズの先頭で停止（Idle）」。
    pub fn new(config: Config) -> Self {
        let config = config.sanitized();
        Self {
            config,
            phase: Phase::Work,
            status: Status::Idle,
            remaining: config.work_secs,
            set_index: 0,
        }
    }

    /// 永続化用の実行状態を取り出す（ADR-0002 §3）。
    pub fn state(&self) -> TimerState {
        TimerState {
            phase: self.phase,
            status: self.status,
            remaining_secs: self.remaining,
            set_index: self.set_index,
        }
    }

    /// 永続化された実行状態から Timer を復元する（プロセス再起動後の復帰用、ADR-0002 §3）。
    ///
    /// `config`（設定の真実）を適用しつつ、phase / status / remaining / set_index を復元する。
    /// `remaining` は壊れた永続値や設定変更（フェーズ短縮）で過大になっても、現フェーズ長へクランプして
    /// 安全側に倒す（`tick` が無限ループしない不変条件を保つ）。復帰直後の壁時計 catch-up は
    /// 呼び出し側（lib.rs）が `tick(elapsed)` で行う。
    pub fn restore(config: Config, state: TimerState) -> Self {
        let config = config.sanitized();
        let phase_len = match state.phase {
            Phase::Work => config.work_secs,
            Phase::Break => config.break_secs,
        };
        // set_index も設定（有限セット数）に対してクランプし、`set_index >= total_sets` という
        // スナップショット不整合（"4/2 セット" 等の表示）を防ぐ。永続後に cycles が縮んでも安全側へ倒す。
        let set_index = match config.cycles.target_sets() {
            Some(target) => state.set_index.min(target.saturating_sub(1)),
            None => state.set_index,
        };
        Self {
            config,
            phase: state.phase,
            status: state.status,
            remaining: state.remaining_secs.min(phase_len),
            set_index,
        }
    }

    /// 起動時に Timer をどう立ち上げるかを、永続セッション・設定・autostart から決める純粋関数
    /// （ADR-0002 §3）。I/O に依存しないので単体テスト可能（lib.rs の起動分岐をここで固定する）。
    ///
    /// - Running 永続（かつ anchor 正常）: 復元し、`now_unix - anchor` の壁時計経過を 1 回の `tick` で
    ///   取り戻す（kill/Doze/画面オフ中の経過を含む catch-up）。跨いだ境界イベントは破棄する
    ///   （過去分の通知音/統計は鳴らさない・数えない＝水増し防止）。
    /// - Paused 永続: そのまま復元（壁時計経過は進めない。autostart は無視）。
    /// - それ以外（セッション無し / Idle 永続 / anchor=0 の異常）: 新規生成し、autostart なら start。
    pub fn restore_for_launch(
        persisted: Option<(TimerState, u64)>,
        config: Config,
        autostart: bool,
        now_unix: u64,
    ) -> Self {
        match persisted {
            Some((state, anchor)) if state.status == Status::Running && anchor != 0 => {
                let mut t = Self::restore(config, state);
                let elapsed = now_unix.saturating_sub(anchor).min(u32::MAX as u64) as u32;
                let _ = t.tick(elapsed);
                t
            }
            Some((state, _)) if state.status == Status::Paused => Self::restore(config, state),
            _ => {
                let mut t = Self::new(config);
                if autostart {
                    t.start();
                }
                t
            }
        }
    }

    /// 現在状態のスナップショットを返す。
    pub fn snapshot(&self) -> TimerSnapshot {
        TimerSnapshot {
            phase: self.phase,
            status: self.status,
            remaining_secs: self.remaining,
            set_index: self.set_index,
            total_sets: self.config.cycles.target_sets(),
            work_secs: self.config.work_secs,
            break_secs: self.config.break_secs,
        }
    }

    /// 現在の設定を返す。
    pub fn config(&self) -> Config {
        self.config
    }

    /// 設定を差し替える。
    ///
    /// 停止中（Idle）は次セッションの先頭に新しい作業時間を反映するため reset する。
    /// 実行中 / 一時停止中は進行中セッションを壊さない: 新しい各フェーズ時間は次フェーズ以降、
    /// サイクル数は次の終了判定から反映される。これにより設定ビュー（ADR-0002）で設定を
    /// 保存しても走行中のタイマーが巻き戻らない。
    pub fn set_config(&mut self, config: Config) {
        self.config = config.sanitized();
        if self.status == Status::Idle {
            self.reset();
        }
    }

    /// 開始 / 再開する。
    /// - Idle からは新しいセッションを最初（作業フェーズ）から始める。
    /// - Paused からは再開する。
    /// - Running のときは何もしない。
    pub fn start(&mut self) {
        match self.status {
            Status::Running => {}
            Status::Paused => self.status = Status::Running,
            Status::Idle => {
                self.phase = Phase::Work;
                self.remaining = self.config.work_secs;
                self.set_index = 0;
                self.status = Status::Running;
            }
        }
    }

    /// 一時停止する（Running のときのみ）。
    pub fn pause(&mut self) {
        if self.status == Status::Running {
            self.status = Status::Paused;
        }
    }

    /// 最初の状態（作業フェーズ先頭・停止）へ戻す。
    pub fn reset(&mut self) {
        self.phase = Phase::Work;
        self.remaining = self.config.work_secs;
        self.set_index = 0;
        self.status = Status::Idle;
    }

    /// 現在フェーズを即座に終了して次へ進める。稼働状態（Running/Paused）は維持する。
    /// Idle のときは何もしない。跨いだフェーズ境界の `TimerEvent` を返す。
    #[must_use = "skip が返すイベントを emit するか、明示的に破棄(let _ =)すること"]
    pub fn skip(&mut self) -> Vec<TimerEvent> {
        if self.status == Status::Idle {
            return Vec::new();
        }
        let mut events = Vec::new();
        self.complete_phase(&mut events);
        events
    }

    /// `secs` 秒だけ時間を進める。Running 以外では何も起きない。
    /// 跨いだフェーズ境界の `TimerEvent` を発生順に返す（システムスリープ等で複数境界を
    /// 一度に跨ぐ場合も正しく処理する）。
    pub fn tick(&mut self, secs: u32) -> Vec<TimerEvent> {
        let mut events = Vec::new();
        if self.status != Status::Running {
            return events;
        }
        let mut left = secs;
        // フェーズ秒数は >= MIN_PHASE_SECS のため、各反復で必ず時間を消費し停止する。
        while left > 0 && self.status == Status::Running {
            if left < self.remaining {
                self.remaining -= left;
                left = 0;
            } else {
                left -= self.remaining;
                self.remaining = 0;
                self.complete_phase(&mut events);
            }
        }
        events
    }

    /// 現在フェーズの完了時遷移。作業→休憩、休憩→次セットの作業 or セッション終了。
    fn complete_phase(&mut self, events: &mut Vec<TimerEvent>) {
        match self.phase {
            Phase::Work => {
                events.push(TimerEvent::WorkEnded);
                self.phase = Phase::Break;
                self.remaining = self.config.break_secs;
            }
            Phase::Break => {
                events.push(TimerEvent::BreakEnded);
                let completed = self.set_index.saturating_add(1);
                if self.config.cycles.is_last_set(completed) {
                    events.push(TimerEvent::SessionFinished);
                    self.reset();
                } else {
                    self.set_index = completed;
                    self.phase = Phase::Work;
                    self.remaining = self.config.work_secs;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(work: u32, brk: u32, cycles: CycleSetting) -> Config {
        Config {
            work_secs: work,
            break_secs: brk,
            cycles,
        }
    }

    #[test]
    fn default_config_is_25_5_and_finite_zero() {
        // 既定が「作業25分=1500秒 / 休憩5分=300秒」であることを生リテラルで固定する。
        let c = Config::default();
        assert_eq!(c.work_secs, 1500);
        assert_eq!(c.break_secs, 300);
        assert_eq!(c.cycles, CycleSetting::Finite(0));
    }

    #[test]
    fn new_timer_starts_idle_at_work() {
        let t = Timer::new(Config::default());
        let s = t.snapshot();
        assert_eq!(s.status, Status::Idle);
        assert_eq!(s.phase, Phase::Work);
        assert_eq!(s.remaining_secs, 1500);
        assert_eq!(s.set_index, 0);
        assert_eq!(s.total_sets, Some(1));
    }

    #[test]
    fn tick_while_idle_does_nothing() {
        let mut t = Timer::new(Config::default());
        let events = t.tick(60);
        assert!(events.is_empty());
        assert_eq!(t.snapshot().remaining_secs, 1500);
        assert_eq!(t.snapshot().status, Status::Idle);
    }

    #[test]
    fn start_then_tick_counts_down() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        let events = t.tick(3);
        assert!(events.is_empty());
        assert_eq!(t.snapshot().remaining_secs, 7);
        assert_eq!(t.snapshot().status, Status::Running);
    }

    #[test]
    fn work_completes_to_break() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        let events = t.tick(10);
        assert_eq!(events, vec![TimerEvent::WorkEnded]);
        let s = t.snapshot();
        assert_eq!(s.phase, Phase::Break);
        assert_eq!(s.remaining_secs, 5);
        assert_eq!(s.set_index, 0);
    }

    #[test]
    fn cycles_zero_runs_exactly_one_set_then_idle() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Finite(0)));
        t.start();
        assert_eq!(t.tick(10), vec![TimerEvent::WorkEnded]);
        let events = t.tick(5);
        assert_eq!(
            events,
            vec![TimerEvent::BreakEnded, TimerEvent::SessionFinished]
        );
        let s = t.snapshot();
        assert_eq!(s.status, Status::Idle);
        assert_eq!(s.phase, Phase::Work);
        assert_eq!(s.set_index, 0);
        assert_eq!(s.remaining_secs, 10);
    }

    #[test]
    fn cycles_zero_and_one_are_equivalent() {
        // どちらも 1 セットで停止する（仕様上の意図的等価）。
        for cycles in [CycleSetting::Finite(0), CycleSetting::Finite(1)] {
            let mut t = Timer::new(config(2, 2, cycles));
            t.start();
            let mut all = t.tick(2);
            all.extend(t.tick(2));
            assert!(all.contains(&TimerEvent::SessionFinished), "{cycles:?}");
            assert_eq!(t.snapshot().status, Status::Idle, "{cycles:?}");
        }
    }

    #[test]
    fn finite_n_runs_n_sets_then_stops() {
        let mut t = Timer::new(config(2, 2, CycleSetting::Finite(3)));
        t.start();
        // 2 セット分（作業+休憩）×2 を消化しても終わらない。
        for expected_set in 0..2u32 {
            assert_eq!(t.tick(2), vec![TimerEvent::WorkEnded]);
            assert_eq!(t.tick(2), vec![TimerEvent::BreakEnded]);
            assert_eq!(t.snapshot().set_index, expected_set + 1);
            assert_eq!(t.snapshot().status, Status::Running);
        }
        // 3 セット目の休憩終了でセッション完了。
        assert_eq!(t.tick(2), vec![TimerEvent::WorkEnded]);
        assert_eq!(
            t.tick(2),
            vec![TimerEvent::BreakEnded, TimerEvent::SessionFinished]
        );
        assert_eq!(t.snapshot().status, Status::Idle);
        assert_eq!(t.snapshot().set_index, 0);
    }

    #[test]
    fn infinite_never_finishes() {
        let mut t = Timer::new(config(1, 1, CycleSetting::Infinite));
        t.start();
        for _ in 0..50 {
            let mut events = t.tick(1); // work end
            events.extend(t.tick(1)); // break end
            assert!(!events.contains(&TimerEvent::SessionFinished));
            assert_eq!(t.snapshot().status, Status::Running);
        }
        assert_eq!(t.snapshot().set_index, 50);
        assert_eq!(t.snapshot().total_sets, None);
    }

    #[test]
    fn pause_stops_countdown_and_start_resumes() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        t.tick(3);
        t.pause();
        assert_eq!(t.snapshot().status, Status::Paused);
        assert!(t.tick(100).is_empty());
        assert_eq!(t.snapshot().remaining_secs, 7); // 進まない
        t.start(); // 再開
        assert_eq!(t.snapshot().status, Status::Running);
        t.tick(7);
        assert_eq!(t.snapshot().phase, Phase::Break);
    }

    #[test]
    fn reset_returns_to_idle_work_head() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Finite(3)));
        t.start();
        t.tick(10); // → break
        t.reset();
        let s = t.snapshot();
        assert_eq!(s.status, Status::Idle);
        assert_eq!(s.phase, Phase::Work);
        assert_eq!(s.remaining_secs, 10);
        assert_eq!(s.set_index, 0);
    }

    #[test]
    fn skip_work_jumps_to_break() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        let events = t.skip();
        assert_eq!(events, vec![TimerEvent::WorkEnded]);
        assert_eq!(t.snapshot().phase, Phase::Break);
        assert_eq!(t.snapshot().remaining_secs, 5);
    }

    #[test]
    fn skip_last_break_finishes_session() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Finite(0)));
        t.start();
        assert_eq!(t.skip(), vec![TimerEvent::WorkEnded]); // → break
        let events = t.skip(); // break → finish
        assert_eq!(
            events,
            vec![TimerEvent::BreakEnded, TimerEvent::SessionFinished]
        );
        assert_eq!(t.snapshot().status, Status::Idle);
    }

    #[test]
    fn skip_while_idle_is_noop() {
        let mut t = Timer::new(Config::default());
        assert!(t.skip().is_empty());
        assert_eq!(t.snapshot().status, Status::Idle);
    }

    #[test]
    fn skip_while_paused_advances_phase_but_stays_paused() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        t.tick(3);
        t.pause();
        let events = t.skip();
        assert_eq!(events, vec![TimerEvent::WorkEnded]);
        let s = t.snapshot();
        assert_eq!(s.phase, Phase::Break);
        assert_eq!(s.remaining_secs, 5);
        assert_eq!(s.status, Status::Paused); // skip しても一時停止は維持
    }

    #[test]
    fn skip_final_break_while_paused_finishes_to_idle() {
        // 最終セットの休憩を Paused 中に skip すると、SessionFinished で Idle へ落ちる
        // （Paused という稼働状態が reset により消える非自明な副作用を固定する）。
        let mut t = Timer::new(config(10, 5, CycleSetting::Finite(0)));
        t.start();
        assert_eq!(t.skip(), vec![TimerEvent::WorkEnded]); // work → break (Running)
        t.pause();
        let events = t.skip(); // break(最終) → finish
        assert_eq!(
            events,
            vec![TimerEvent::BreakEnded, TimerEvent::SessionFinished]
        );
        assert_eq!(t.snapshot().status, Status::Idle);
    }

    #[test]
    fn start_while_running_is_noop() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        t.tick(3);
        t.start(); // 進行中の start は無視され、巻き戻らない
        assert_eq!(t.snapshot().remaining_secs, 7);
        assert_eq!(t.snapshot().status, Status::Running);
    }

    #[test]
    fn tick_across_multiple_boundaries_in_one_call() {
        // work=2, break=1, 2 セット。10 秒一気に進めて完了まで到達する。
        let mut t = Timer::new(config(2, 1, CycleSetting::Finite(2)));
        t.start();
        let events = t.tick(10);
        // 1set: WorkEnded,BreakEnded / 2set: WorkEnded,BreakEnded+SessionFinished
        assert_eq!(
            events,
            vec![
                TimerEvent::WorkEnded,
                TimerEvent::BreakEnded,
                TimerEvent::WorkEnded,
                TimerEvent::BreakEnded,
                TimerEvent::SessionFinished,
            ]
        );
        assert_eq!(t.snapshot().status, Status::Idle);
        // 完了後は Idle なので、余った時間は消費されず巻き込まれない。
        assert_eq!(t.snapshot().remaining_secs, 2);
    }

    #[test]
    fn zero_duration_config_is_clamped_and_safe() {
        // 0 秒設定は最小 1 秒に丸められ、tick が無限ループしない。
        let mut t = Timer::new(config(0, 0, CycleSetting::Infinite));
        let s = t.snapshot();
        assert_eq!(s.work_secs, MIN_PHASE_SECS);
        assert_eq!(s.break_secs, MIN_PHASE_SECS);
        t.start();
        let events = t.tick(3); // 3 秒で複数境界を跨ぐが停止する
        assert!(events.contains(&TimerEvent::WorkEnded));
    }

    #[test]
    fn snapshot_total_sets_reflects_setting() {
        assert_eq!(
            Timer::new(config(1, 1, CycleSetting::Finite(0)))
                .snapshot()
                .total_sets,
            Some(1)
        );
        assert_eq!(
            Timer::new(config(1, 1, CycleSetting::Finite(4)))
                .snapshot()
                .total_sets,
            Some(4)
        );
        assert_eq!(
            Timer::new(config(1, 1, CycleSetting::Infinite))
                .snapshot()
                .total_sets,
            None
        );
    }

    #[test]
    fn snapshot_serializes_to_camel_case_keys() {
        // フロント timer.ts はこれらのキー名に依存する。Rust 側を変えたら timer.ts も同期すること。
        let t = Timer::new(config(1, 1, CycleSetting::Finite(2)));
        let json = serde_json::to_value(t.snapshot()).unwrap();
        let obj = json.as_object().expect("snapshot is a JSON object");
        for key in [
            "phase",
            "status",
            "remainingSecs",
            "setIndex",
            "totalSets",
            "workSecs",
            "breakSecs",
        ] {
            assert!(obj.contains_key(key), "missing snapshot key: {key}");
        }
        // フェーズ/状態は lowercase でシリアライズされる契約。
        assert_eq!(json["phase"], "work");
        assert_eq!(json["status"], "idle");
    }

    #[test]
    fn state_restore_round_trips_running_session() {
        // 走行中の状態を取り出して同じ config で復元すると、snapshot が一致する（ADR-0002 §3）。
        let mut t = Timer::new(config(100, 30, CycleSetting::Finite(3)));
        t.start();
        t.tick(40); // 作業フェーズ、残り 60
        let restored = Timer::restore(t.config(), t.state());
        assert_eq!(restored.snapshot(), t.snapshot());
        assert_eq!(restored.snapshot().status, Status::Running);
        assert_eq!(restored.snapshot().remaining_secs, 60);
    }

    #[test]
    fn restore_then_tick_catches_up_wall_elapsed() {
        // 復元後に「停止していた間の経過秒」を 1 回の tick で与えると、正しく進む（kill/Doze からの復帰）。
        let mut t = Timer::new(config(100, 30, CycleSetting::Infinite));
        t.start();
        t.tick(40); // 残り 60
        let mut restored = Timer::restore(t.config(), t.state());
        // プロセス停止中に 70 秒経過 → 作業(残り60)を越えて休憩へ入り、残り 20。
        let events = restored.tick(70);
        assert_eq!(events, vec![TimerEvent::WorkEnded]);
        assert_eq!(restored.snapshot().phase, Phase::Break);
        assert_eq!(restored.snapshot().remaining_secs, 20);
    }

    #[test]
    fn restore_clamps_remaining_to_phase_length() {
        // 設定が縮んだ（あるいは壊れた永続値の）場合でも、remaining は現フェーズ長へクランプされる。
        let state = TimerState {
            phase: Phase::Work,
            status: Status::Running,
            remaining_secs: 9999,
            set_index: 0,
        };
        let restored = Timer::restore(config(60, 10, CycleSetting::Infinite), state);
        assert_eq!(restored.snapshot().remaining_secs, 60); // work_secs へクランプ
    }

    #[test]
    fn restore_clamps_set_index_to_cycles() {
        // 永続後に cycles が縮んでも、set_index は total_sets-1 へクランプされ "9/3" のような不整合を防ぐ。
        let state = TimerState {
            phase: Phase::Break,
            status: Status::Running,
            remaining_secs: 5,
            set_index: 9,
        };
        let t = Timer::restore(config(60, 10, CycleSetting::Finite(3)), state);
        assert_eq!(t.snapshot().set_index, 2); // target 3 → 最大 index 2
        assert!(t.snapshot().set_index < t.snapshot().total_sets.unwrap());
    }

    #[test]
    fn restore_for_launch_running_catches_up() {
        let state = TimerState {
            phase: Phase::Work,
            status: Status::Running,
            remaining_secs: 100,
            set_index: 0,
        };
        // 40 秒経過 → 100 - 40 = 60。
        let t = Timer::restore_for_launch(
            Some((state, 1000)),
            config(120, 30, CycleSetting::Infinite),
            false,
            1040,
        );
        assert_eq!(t.snapshot().status, Status::Running);
        assert_eq!(t.snapshot().remaining_secs, 60);
    }

    #[test]
    fn restore_for_launch_paused_ignores_autostart_and_elapsed() {
        let state = TimerState {
            phase: Phase::Break,
            status: Status::Paused,
            remaining_secs: 15,
            set_index: 1,
        };
        let t = Timer::restore_for_launch(
            Some((state, 1000)),
            config(120, 30, CycleSetting::Finite(3)),
            true, // autostart は Paused 復元では無視される
            9_999_999,
        );
        assert_eq!(t.snapshot().status, Status::Paused);
        assert_eq!(t.snapshot().remaining_secs, 15);
    }

    #[test]
    fn restore_for_launch_idle_persisted_with_autostart_starts() {
        let state = TimerState {
            phase: Phase::Work,
            status: Status::Idle,
            remaining_secs: 120,
            set_index: 0,
        };
        let t = Timer::restore_for_launch(
            Some((state, 1000)),
            config(120, 30, CycleSetting::Infinite),
            true,
            2000,
        );
        assert_eq!(t.snapshot().status, Status::Running); // autostart 発火
    }

    #[test]
    fn restore_for_launch_no_session_respects_autostart() {
        let idle = Timer::restore_for_launch(None, config(120, 30, CycleSetting::Infinite), false, 2000);
        assert_eq!(idle.snapshot().status, Status::Idle);
        assert_eq!(idle.snapshot().remaining_secs, 120);
        let started = Timer::restore_for_launch(None, config(120, 30, CycleSetting::Infinite), true, 2000);
        assert_eq!(started.snapshot().status, Status::Running);
    }

    #[test]
    fn restore_for_launch_running_clock_backwards_does_not_advance() {
        // anchor が now より未来（時計巻き戻し/NTP）でも saturating で経過 0 → 巻き戻らない。
        let state = TimerState {
            phase: Phase::Work,
            status: Status::Running,
            remaining_secs: 80,
            set_index: 0,
        };
        let t = Timer::restore_for_launch(
            Some((state, 5000)), // anchor は now より未来
            config(120, 30, CycleSetting::Infinite),
            false,
            4000, // now < anchor
        );
        assert_eq!(t.snapshot().status, Status::Running);
        assert_eq!(t.snapshot().remaining_secs, 80); // 進まない
    }

    #[test]
    fn restore_for_launch_anchor_zero_running_falls_back_to_fresh() {
        // anchor=0（保存時に壁時計取得失敗）の Running は異常として通常起動へフォールバックし、
        // 巨大な elapsed でセッションが勝手に飛ぶのを防ぐ。
        let state = TimerState {
            phase: Phase::Work,
            status: Status::Running,
            remaining_secs: 50,
            set_index: 0,
        };
        let t = Timer::restore_for_launch(
            Some((state, 0)),
            config(120, 30, CycleSetting::Infinite),
            false,
            9_999_999,
        );
        assert_eq!(t.snapshot().status, Status::Idle); // フォールバック
        assert_eq!(t.snapshot().remaining_secs, 120);
    }

    #[test]
    fn restore_paused_does_not_advance_without_tick() {
        let state = TimerState {
            phase: Phase::Break,
            status: Status::Paused,
            remaining_secs: 15,
            set_index: 1,
        };
        let restored = Timer::restore(config(100, 30, CycleSetting::Finite(3)), state);
        let s = restored.snapshot();
        assert_eq!(s.status, Status::Paused);
        assert_eq!(s.phase, Phase::Break);
        assert_eq!(s.remaining_secs, 15);
        assert_eq!(s.set_index, 1);
    }

    #[test]
    fn timer_state_serializes_to_camel_case_keys() {
        // session.json の内部契約（restore で読み戻すキー名）を固定する。
        let t = Timer::new(config(100, 30, CycleSetting::Finite(2)));
        let json = serde_json::to_value(t.state()).unwrap();
        let obj = json.as_object().expect("state is a JSON object");
        for key in ["phase", "status", "remainingSecs", "setIndex"] {
            assert!(obj.contains_key(key), "missing state key: {key}");
        }
        // round-trip でデシリアライズできる。
        let back: TimerState = serde_json::from_value(json).unwrap();
        assert_eq!(back, t.state());
    }

    #[test]
    fn set_config_while_idle_applies_and_resets() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.set_config(config(30, 10, CycleSetting::Finite(2)));
        let s = t.snapshot();
        assert_eq!(s.status, Status::Idle);
        assert_eq!(s.remaining_secs, 30); // 次セッション先頭に新しい作業時間が反映
        assert_eq!(s.work_secs, 30);
        assert_eq!(s.break_secs, 10);
        assert_eq!(s.total_sets, Some(2));
    }

    #[test]
    fn set_config_while_running_keeps_session_and_applies_to_future() {
        let mut t = Timer::new(config(10, 5, CycleSetting::Infinite));
        t.start();
        t.tick(3); // 作業フェーズ、残り 7
        t.set_config(config(30, 8, CycleSetting::Finite(2)));
        let s = t.snapshot();
        // 進行中セッションは壊さない: 現フェーズの残り時間・状態は保持される。
        assert_eq!(s.status, Status::Running);
        assert_eq!(s.phase, Phase::Work);
        assert_eq!(s.remaining_secs, 7);
        // 新しい時間は次フェーズ以降に反映: 現作業を skip すると休憩は新しい 8 秒。
        assert_eq!(t.skip(), vec![TimerEvent::WorkEnded]);
        assert_eq!(t.snapshot().remaining_secs, 8);
        // サイクル数は即時反映される。
        assert_eq!(t.snapshot().total_sets, Some(2));
    }
}
