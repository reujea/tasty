import { readFileSync } from "node:fs";

/** pages.yml writes release.json before building. If absent, download links
 * fall back to the releases page. */
export type Platform = "macos" | "windows" | "linux";
export type Arch = "x64" | "arm64";

export interface Download {
  platform: Platform;
  /** CPU architecture of the asset. The landing button only links an asset
   * whose architecture matches the one the visitor's browser reports. */
  arch: Arch;
  /** Direct asset URL, or undefined when the release has no asset for it. */
  href?: string;
}

/** Which asset belongs to which platform and architecture, by file-name
 * suffix. macOS ships Apple silicon only. */
const ASSETS: ReadonlyArray<readonly [Platform, Arch, string]> = [
  ["macos", "arm64", "-macos-arm64.dmg"],
  ["windows", "x64", "-windows-x64.msi"],
  ["windows", "arm64", "-windows-arm64.msi"],
  ["linux", "x64", "-x86_64.AppImage"],
  ["linux", "arm64", "-aarch64.AppImage"],
];

export const RELEASES_URL = "https://github.com/zilhak/tasty/releases/latest";

interface Manifest {
  tag?: string;
  assets: Map<string, string>;
}

function load(siteDir: string): Manifest {
  let raw: string;
  try {
    raw = readFileSync(`${siteDir}/release.json`, "utf8");
  } catch {
    return { assets: new Map() };
  }
  try {
    const json = JSON.parse(raw) as { tagName?: string; assets?: { name?: string; url?: string }[] };
    const assets = new Map<string, string>();
    for (const a of json.assets ?? []) if (a.name && a.url) assets.set(a.name, a.url);
    return { tag: json.tagName, assets };
  } catch (e) {
    console.warn(`site/release.json is not valid JSON, ignoring it: ${e}`);
    return { assets: new Map() };
  }
}

/** Version of the latest published release, without the "v" prefix. The site
 * shows this one because it is the version visitors can download. Undefined
 * when release.json is absent, as in a local build. */
export function releaseVersion(siteDir: string): string | undefined {
  return load(siteDir).tag?.replace(/^v/, "");
}

export function downloads(siteDir: string): { tag?: string; items: Download[] } {
  const { tag, assets } = load(siteDir);
  const items = ASSETS.map(([platform, arch, suffix]) => {
    const hit = [...assets].find(([name]) => name.endsWith(suffix));
    return { platform, arch, href: hit?.[1] };
  });
  return { tag, items };
}
