// Copyright (C) 2026 Eduardo Marques Santos
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

import {
  createHash,
  createPrivateKey,
  createPublicKey,
  generateKeyPairSync,
  sign,
  verify,
} from "node:crypto";
import { copyFile, readdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

const [targetDir = "target/release-archives"] = process.argv.slice(2);
const releaseDirectory = resolve(targetDir);

const entries = await readdir(releaseDirectory);
const packageFiles = entries
  .filter((file) => {
    if (file === "SHA256SUMS" || file.startsWith("SHA256SUMS.") || file === "release-key.pub") {
      return false;
    }
    return (
      file.endsWith(".tar.gz") ||
      file.endsWith(".deb") ||
      file.endsWith(".apk") ||
      file.endsWith(".pkg.tar.zst") ||
      file.endsWith(".vsix")
    );
  })
  .toSorted();

if (packageFiles.length === 0) {
  throw new Error(`No release packages found to sign in ${releaseDirectory}`);
}

const lines = [];
for (const file of packageFiles) {
  const content = await readFile(resolve(releaseDirectory, file));
  const digest = createHash("sha256").update(content).digest("hex");
  lines.push(`${digest}  ${file}`);
}
const checksumManifest = `${lines.join("\n")}\n`;
const checksumPath = resolve(releaseDirectory, "SHA256SUMS");
await writeFile(checksumPath, checksumManifest, { mode: 0o644 });

const privateKeyFile = process.env.RELEASE_SIGNING_KEY_FILE;
const privateKeyString =
  process.env.EVERYINIT_RELEASE_SIGNING_KEY || process.env.RELEASE_SIGNING_KEY;
const pinnedPublicKey = (await readFile("infra/get/release-key.pub", "utf8")).trim();

let privateKey;
let isEphemeral = false;

if (privateKeyString) {
  privateKey = createPrivateKey(privateKeyString);
} else if (privateKeyFile) {
  privateKey = createPrivateKey(await readFile(resolve(privateKeyFile)));
} else {
  const pair = generateKeyPairSync("ed25519");
  privateKey = pair.privateKey;
  isEphemeral = true;
  console.log(
    "No EVERYINIT_RELEASE_SIGNING_KEY or RELEASE_SIGNING_KEY provided; signing with ephemeral Ed25519 key for rehearsal verification.",
  );
}

if (privateKey.asymmetricKeyType !== "ed25519") {
  throw new Error("Release signing key must be Ed25519");
}

const signingPublicKey = createPublicKey(privateKey)
  .export({ type: "spki", format: "pem" })
  .toString()
  .trim();

if (!isEphemeral && signingPublicKey !== pinnedPublicKey) {
  throw new Error("Release signing key does not match infra/get/release-key.pub");
}

const signature = sign(null, Buffer.from(checksumManifest), privateKey);

if (!verify(null, Buffer.from(checksumManifest), createPublicKey(privateKey), signature)) {
  throw new Error("Generated release signature failed in-memory verification");
}

await writeFile(resolve(releaseDirectory, "SHA256SUMS.sig"), signature, { mode: 0o644 });
await writeFile(
  resolve(releaseDirectory, "SHA256SUMS.sig.b64"),
  `${signature.toString("base64")}\n`,
  { mode: 0o644 },
);

if (isEphemeral) {
  await writeFile(resolve(releaseDirectory, "release-key.pub"), `${signingPublicKey}\n`, {
    mode: 0o644,
  });
} else {
  await copyFile("infra/get/release-key.pub", resolve(releaseDirectory, "release-key.pub"));
}

console.log(
  `Generated and cryptographically signed SHA256SUMS for ${packageFiles.length} artifacts in ${releaseDirectory}:`,
);
for (const file of packageFiles) {
  console.log(`  - ${file}`);
}
