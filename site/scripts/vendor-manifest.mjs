/**
 * Writes site/vendor/MANIFEST: the remote sha256 of every vendored file, and
 * the local sha256 plus a reason for each file kept different on purpose.
 *
 *   node scripts/vendor-manifest.mjs --remote <dir> [--reason <path>=<text>]... [--check]
 *
 * <dir> holds the files fetched from the remote project in this re-vendor,
 * laid out at their remote paths without the leading `project/` (see remotePathOf).
 * A file whose local bytes differ from the remote keeps the reason already in
 * MANIFEST, or takes one from --reason; without either the script fails and
 * writes nothing. --check compares instead of writing and fails on a change.
 */
import { createHash } from "node:crypto";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const vendor = resolve(dirname(fileURLToPath(import.meta.url)), "../vendor");
const manifestPath = join(vendor, "MANIFEST");
// Files that describe the copy rather than belong to it.
const OWN_FILES = new Set(["README.md", "MANIFEST"]);

const HEADER = [
  "# site/vendor 사본의 원격 원본 해시. site/scripts/vendor-manifest.mjs가 쓴다.",
  "# 형식: <경로> TAB <원격 sha256>",
  "#   또는 <경로> TAB <원격 sha256> TAB variant TAB <로컬 sha256> TAB <이유>",
  "# 경로는 site/vendor 기준이며 screens/는 원격 assets/screens/에 대응한다.",
  "# 해시는 손으로 고치지 않는다. 이유 열은 고칠 수 있다.",
  "# 검사: crates/tasty-doc-guards/tests/site_vendor_manifest_matches.rs",
];

function fail(msg) {
  console.error(msg);
  process.exit(1);
}

function parseArgs(argv) {
  const out = { remote: null, reasons: new Map(), localVariants: new Map(), check: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--remote") out.remote = argv[++i];
    else if (a === "--check") out.check = true;
    else if (a === "--local-variant") {
      const spec = argv[++i] ?? "";
      const eq = spec.indexOf("=");
      if (eq <= 0 || !spec.slice(eq + 1).trim()) fail(`--local-variant needs <path>=<reason>: ${spec}`);
      out.localVariants.set(spec.slice(0, eq), spec.slice(eq + 1).trim());
    } else if (a === "--reason") {
      const spec = argv[++i] ?? "";
      const eq = spec.indexOf("=");
      if (eq <= 0 || !spec.slice(eq + 1).trim()) fail(`--reason needs <path>=<text>: ${spec}`);
      out.reasons.set(spec.slice(0, eq), spec.slice(eq + 1).trim());
    } else fail(`unknown argument: ${a}`);
  }
  if (out.remote && out.localVariants.size) fail("--remote and --local-variant are exclusive");
  if (!out.remote && !out.localVariants.size) fail("usage: vendor-manifest.mjs --remote <dir> [--reason <path>=<text>]... [--check]");
  return out;
}

function walk(dir, root, acc) {
  for (const name of readdirSync(dir).sort()) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, root, acc);
    else acc.push(relative(root, p).replace(/\\/g, "/"));
  }
  return acc;
}

const sha256 = (path) => createHash("sha256").update(readFileSync(path)).digest("hex");
// The Artifact keeps some files at other paths than the copy does.
const remotePathOf = (local) => {
  if (local.startsWith("screens/")) return `assets/${local}`;
  if (local === "guidelines/brand-logo.html") return `archived/${local}`;
  const typings = local.match(/^components\/[^/]+\/([^/]+)\.d\.ts$/);
  return typings ? `components/${typings[1]}/${typings[1]}.d.ts` : local;
};

// Reasons recorded by the previous run, keyed by local path.
function recordedReasons() {
  const reasons = new Map();
  if (!existsSync(manifestPath)) return reasons;
  for (const line of readFileSync(manifestPath, "utf8").split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const f = line.split("\t");
    if (f.length === 5 && f[2] === "variant") reasons.set(f[0], f[4]);
  }
  return reasons;
}

// A documented local reference correction does not fetch a new remote snapshot.
// Preserve the recorded remote hash and compute only the explicitly named variant hashes.
function refreshLocalVariants(args) {
  const original = readFileSync(manifestPath, "utf8");
  const pending = new Map(args.localVariants);
  const lines = original.split("\n").map((line) => {
    if (!line || line.startsWith("#")) return line;
    const fields = line.split("\t");
    if (!(fields.length === 2 || (fields.length === 5 && fields[2] === "variant"))) fail(`invalid manifest row: ${fields[0]}`);
    if (!/^[a-f0-9]{64}$/.test(fields[1])) fail(`invalid recorded remote hash: ${fields[0]}`);
    const reason = pending.get(fields[0]);
    if (reason === undefined) return line;
    const path = resolve(vendor, fields[0]);
    if (relative(vendor, path).startsWith("..") || OWN_FILES.has(fields[0])) fail(`invalid local variant: ${fields[0]}`);
    if (/[\t\r\n]/.test(reason)) fail("variant reason must be one line");
    pending.delete(fields[0]);
    const local = sha256(path);
    return local === fields[1] ? fields.slice(0, 2).join("\t") : [fields[0], fields[1], "variant", local, reason].join("\t");
  });
  if (pending.size) fail(`local variant absent from recorded manifest: ${[...pending.keys()].join(", ")}`);
  const updated = lines.join("\n");
  if (args.check) {
    if (updated !== original) fail("recorded local variant hashes differ");
  } else {
    writeFileSync(manifestPath, updated);
  }
  console.log(`Local reference variants: ${args.localVariants.size}; recorded remote hashes preserved`);
}

const args = parseArgs(process.argv.slice(2));
if (args.localVariants.size) {
  refreshLocalVariants(args);
  process.exit(0);
}
const remoteDir = resolve(args.remote);
if (!existsSync(remoteDir)) fail(`no remote directory at ${remoteDir}`);

// Whole paths in byte order: site_vendor_manifest_matches.rs reads this order, and a per-folder walk puts "a/" before "a-b/".
const locals = walk(vendor, vendor, []).filter((p) => !OWN_FILES.has(p)).sort();
const reasons = recordedReasons();
for (const [p, r] of args.reasons) reasons.set(p, r);

const lines = [];
const missingRemote = [];
const needReason = [];
for (const path of locals) {
  const remoteFile = join(remoteDir, remotePathOf(path));
  if (!existsSync(remoteFile)) {
    missingRemote.push(path);
    continue;
  }
  const remote = sha256(remoteFile);
  const local = sha256(join(vendor, path));
  if (remote === local) {
    lines.push(`${path}\t${remote}`);
    continue;
  }
  const reason = reasons.get(path);
  if (!reason) needReason.push(path);
  else lines.push(`${path}\t${remote}\tvariant\t${local}\t${reason}`);
}

const remoteOnly = walk(remoteDir, remoteDir, []).filter(
  (p) => !locals.some((l) => remotePathOf(l) === p),
);
if (remoteOnly.length) {
  console.warn(`fetched but not vendored (excluded or new — check README "원본에서 제외한 것"):`);
  for (const p of remoteOnly) console.warn(`  ${p}`);
}
if (missingRemote.length || needReason.length) {
  for (const p of missingRemote) console.error(`not fetched from the remote: ${p}`);
  for (const p of needReason) console.error(`differs from the remote with no reason: ${p} (pass --reason ${p}=<text>, or fetch it again)`);
  fail("MANIFEST not written.");
}

const text = `${HEADER.join("\n")}\n${lines.join("\n")}\n`;
const variants = lines.filter((l) => l.includes("\tvariant\t")).length;
if (args.check) {
  const current = existsSync(manifestPath) ? readFileSync(manifestPath, "utf8") : "";
  if (current !== text) fail("MANIFEST differs from the remote files given.");
  console.log(`MANIFEST matches: ${lines.length} files, ${variants} variants.`);
} else {
  writeFileSync(manifestPath, text);
  console.log(`MANIFEST written: ${lines.length} files, ${variants} variants.`);
}
