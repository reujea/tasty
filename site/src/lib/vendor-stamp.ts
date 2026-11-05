import { execFileSync } from "node:child_process";
import { resolve } from "node:path";

/** Date of the latest change to the vendored design copy. README and MANIFEST describe the copy, so they do not count.
 *  Missing history or a failed Git command gives no stamp, and the page shows no date. */
export const vendorStamp = (): { date: string; commit: string } | null => {
  try {
    const out = execFileSync(
      "git",
      ["log", "-1", "--format=%cs\t%h", "--", "site/vendor", ":(exclude)site/vendor/README.md", ":(exclude)site/vendor/MANIFEST"],
      { cwd: resolve(process.cwd(), ".."), encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] },
    ).trim();
    if (!out) return null;
    const [date, commit] = out.split("\t");
    return /^\d{4}-\d{2}-\d{2}$/.test(date) ? { date, commit } : null;
  } catch {
    return null;
  }
};
