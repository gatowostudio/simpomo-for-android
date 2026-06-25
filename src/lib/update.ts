// 手動「更新を確認」。GitHub Releases の最新版とアプリのバージョンを比べ、新しければリリースページを
// 既定ブラウザで開く。署名鍵不要・自動インストールなし・通信は確認ボタンを押した時のみ
// （オフライン完結方針を壊さない軽量な方式）。検知対象は「公開済み(published)リリース」のみ
// （CI は下書きで作るので、配布＝Publish した版だけが対象。prerelease は対象外）。
//
// fork / 改名する場合は下の REPO を変更すること（README のリリース URL も合わせて更新）。
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";

const REPO = "gatowostudio/simpomo-for-android";
const RELEASES_PAGE = `https://github.com/${REPO}/releases`;
const TIMEOUT_MS = 8000;

export interface UpdateInfo {
  current: string;
  /** 最新の公開版（無ければ ""）。先頭の v は除去済み。 */
  latest: string;
  newer: boolean;
}

/** "1.2.3" 等を数値配列に。先頭の v とプレリリース接尾辞(-rc.1 等)は無視する。 */
function parseVersion(v: string): number[] {
  return v
    .replace(/^v/, "")
    .split("-")[0]
    .split(".")
    .map((p) => parseInt(p, 10) || 0);
}

function isNewer(latest: string, current: string): boolean {
  const a = parseVersion(latest);
  const b = parseVersion(current);
  const len = Math.max(a.length, b.length);
  for (let i = 0; i < len; i++) {
    const x = a[i] ?? 0;
    const y = b[i] ?? 0;
    if (x !== y) return x > y;
  }
  return false;
}

function fetchLatest(): Promise<Response> {
  // タイムアウトを設けて「Checking…」のまま固着するのを防ぐ。
  const ctrl = new AbortController();
  const id = setTimeout(() => ctrl.abort(), TIMEOUT_MS);
  return fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
    headers: { Accept: "application/vnd.github+json" },
    signal: ctrl.signal,
  }).finally(() => clearTimeout(id));
}

/** GitHub Releases の最新公開版を取得し、現在版と比較する。 */
export async function checkForUpdate(): Promise<UpdateInfo> {
  const current = await getVersion();

  let res: Response;
  try {
    res = await fetchLatest();
  } catch (e) {
    if (e instanceof DOMException && e.name === "AbortError") {
      throw new Error("Network timeout. Try again.");
    }
    throw new Error("Network error. Check your connection.");
  }

  // 公開済みリリースが無いと 404。エラーではなく「リリースなし」(latest 空)として扱う。
  if (res.status === 404) {
    return { current, latest: "", newer: false };
  }
  if (res.status === 403) {
    throw new Error("Rate limited by GitHub. Try again later.");
  }
  if (!res.ok) {
    throw new Error(`GitHub API ${res.status}`);
  }
  const data = await res.json();
  const latest = String(data.tag_name ?? "").replace(/^v/, "");
  return { current, latest, newer: isNewer(latest, current) };
}

/** リリース一覧ページを既定ブラウザで開く。 */
export const openReleases = (): Promise<void> => openUrl(RELEASES_PAGE);
