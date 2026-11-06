import { generateFonts, FontAssetType } from "fantasticon";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import * as os from "node:os";

const vscodeRoot = path.resolve(import.meta.dirname, "..");
const assetsDir = path.join(vscodeRoot, "assets");
const logoSvgPath = path.join(assetsDir, "logo-dark.svg");
const fontOutputPath = path.join(assetsDir, "everyinit-icons.woff");

async function buildIconFont() {
  const svgContent = await fs.readFile(logoSvgPath, "utf8");
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "everyinit-font-build-"));
  const inputDir = path.join(tmpDir, "icons");
  const outputDir = path.join(tmpDir, "dist");

  await fs.mkdir(inputDir, { recursive: true });
  await fs.mkdir(outputDir, { recursive: true });

  // Use pure black fill for clean font glyph generation
  const fontSvg = svgContent.replace(/#1ED4BE/gi, "#000000");
  await fs.writeFile(path.join(inputDir, "everyinit.svg"), fontSvg, "utf8");

  try {
    await generateFonts({
      inputDir,
      outputDir,
      name: "everyinit-icons",
      fontTypes: [FontAssetType.WOFF],
      assetTypes: [],
      codepoints: {
        everyinit: 0xe001,
      },
      formatOptions: {
        woff: {},
      },
    });

    const generatedWoff = path.join(outputDir, "everyinit-icons.woff");
    const fontData = await fs.readFile(generatedWoff);
    await fs.writeFile(fontOutputPath, fontData);

    const stats = await fs.stat(fontOutputPath);
    console.log(
      `Successfully built icon font: ${path.relative(vscodeRoot, fontOutputPath)} (${stats.size} bytes, glyph 'everyinit' at \\E001)`
    );
  } finally {
    await fs.rm(tmpDir, { recursive: true, force: true });
  }
}

buildIconFont().catch((err) => {
  console.error("Failed to build icon font:", err);
  process.exit(1);
});

