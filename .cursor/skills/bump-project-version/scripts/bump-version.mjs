import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = process.cwd();
const paths = {
  packageJson: resolve(root, "package.json"),
  packageLock: resolve(root, "package-lock.json"),
  cargoToml: resolve(root, "src-tauri/Cargo.toml"),
  cargoLock: resolve(root, "src-tauri/Cargo.lock"),
  tauriConfig: resolve(root, "src-tauri/tauri.conf.json"),
  readme: resolve(root, "README.md"),
};

const read = (path) => readFileSync(path, "utf8");
const packageJsonText = read(paths.packageJson);
const packageLockText = read(paths.packageLock);
const cargoTomlText = read(paths.cargoToml);
const cargoLockText = read(paths.cargoLock);
const tauriConfigText = read(paths.tauriConfig);
const readmeText = read(paths.readme);

const packageJson = JSON.parse(packageJsonText);
const packageLock = JSON.parse(packageLockText);
const tauriConfig = JSON.parse(tauriConfigText);
const cargoTomlVersion = cargoTomlText.match(
  /^\[package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
)?.[1];
const cargoLockVersion = cargoLockText.match(
  /^\[\[package\]\]\r?\nname = "stock-widget"\r?\nversion = "([^"]+)"/m,
)?.[1];

const versions = {
  "package.json": packageJson.version,
  "package-lock.json": packageLock.version,
  "package-lock.json root package": packageLock.packages?.[""]?.version,
  "src-tauri/Cargo.toml": cargoTomlVersion,
  "src-tauri/Cargo.lock": cargoLockVersion,
  "src-tauri/tauri.conf.json": tauriConfig.version,
};

if (Object.values(versions).some((version) => !version)) {
  throw new Error(`Unable to read all version sources:\n${JSON.stringify(versions, null, 2)}`);
}

const uniqueVersions = new Set(Object.values(versions));
if (uniqueVersions.size !== 1) {
  throw new Error(`Version sources disagree:\n${JSON.stringify(versions, null, 2)}`);
}

const current = packageJson.version;
const semverMatch = current.match(/^(\d+)\.(\d+)\.(\d+)$/);
if (!semverMatch) {
  throw new Error(`Current version is not a supported x.y.z version: ${current}`);
}

const requested = (process.argv[2] ?? "patch").replace(/^v/, "");
let next;

if (["patch", "minor", "major"].includes(requested)) {
  let [, major, minor, patch] = semverMatch.map(Number);
  if (requested === "major") {
    major += 1;
    minor = 0;
    patch = 0;
  } else if (requested === "minor") {
    minor += 1;
    patch = 0;
  } else {
    patch += 1;
  }
  next = `${major}.${minor}.${patch}`;
} else if (/^\d+\.\d+\.\d+$/.test(requested)) {
  next = requested;
} else {
  throw new Error("Usage: node bump-version.mjs [x.y.z|patch|minor|major]");
}

if (next === current) {
  throw new Error(`Version is already ${current}`);
}

function replaceOnce(text, pattern, replacement, label) {
  const matches = text.match(new RegExp(pattern.source, pattern.flags.includes("g") ? pattern.flags : `${pattern.flags}g`));
  if (matches?.length !== 1) {
    throw new Error(`Expected one ${label} match, found ${matches?.length ?? 0}`);
  }
  return text.replace(pattern, replacement);
}

const escapedCurrent = current.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const edits = new Map();

edits.set(
  paths.packageJson,
  replaceOnce(
    packageJsonText,
    new RegExp(`^(\\s*"version"\\s*:\\s*)"${escapedCurrent}"`, "m"),
    `$1"${next}"`,
    "package.json version",
  ),
);

let updatedPackageLock = replaceOnce(
  packageLockText,
  new RegExp(`^(  "version"\\s*:\\s*)"${escapedCurrent}"`, "m"),
  `$1"${next}"`,
  "package-lock.json top-level version",
);
updatedPackageLock = replaceOnce(
  updatedPackageLock,
  new RegExp(`^(      "version"\\s*:\\s*)"${escapedCurrent}"`, "m"),
  `$1"${next}"`,
  "package-lock.json root package version",
);
edits.set(paths.packageLock, updatedPackageLock);

edits.set(
  paths.cargoToml,
  replaceOnce(
    cargoTomlText,
    new RegExp(`^(\\[package\\][\\s\\S]*?^version\\s*=\\s*)"${escapedCurrent}"`, "m"),
    `$1"${next}"`,
    "Cargo.toml package version",
  ),
);

edits.set(
  paths.cargoLock,
  replaceOnce(
    cargoLockText,
    new RegExp(`(^\\[\\[package\\]\\]\\r?\\nname = "stock-widget"\\r?\\nversion = )"${escapedCurrent}"`, "m"),
    `$1"${next}"`,
    "Cargo.lock stock-widget version",
  ),
);

edits.set(
  paths.tauriConfig,
  replaceOnce(
    tauriConfigText,
    new RegExp(`^(\\s*"version"\\s*:\\s*)"${escapedCurrent}"`, "m"),
    `$1"${next}"`,
    "tauri.conf.json version",
  ),
);

const oldTag = `v${current}`;
const newTag = `v${next}`;
edits.set(paths.readme, readmeText.split(oldTag).join(newTag));

for (const [path, content] of edits) {
  writeFileSync(path, content, "utf8");
}

console.log(`Version updated: ${current} -> ${next}`);
for (const path of edits.keys()) {
  console.log(`- ${path.slice(root.length + 1)}`);
}
