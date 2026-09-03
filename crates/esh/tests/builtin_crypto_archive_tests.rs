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

use esh::{Engine, ExecutionContext};
use std::fs;
use tempfile::tempdir;

#[tokio::test]
async fn test_hash_and_base64() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        hash --sha256 "hello world"
        base64 "my-secret-token"
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);
    let output = ctx.get_captured_stdout().unwrap();
    // sha256 of "hello world" is b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
    assert!(output.contains("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"));
    // base64 of "my-secret-token" is bXktc2VjcmV0LXRva2Vu
    assert!(output.contains("bXktc2VjcmV0LXRva2Vu"));
}

#[tokio::test]
async fn test_archive_and_unarchive_tar_and_zip() {
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("dist");
    let _out_tar = tmp.path().join("bundle.tar.gz");
    let _out_zip = tmp.path().join("bundle.zip");
    let extract_tar = tmp.path().join("extracted_tar");
    let extract_zip = tmp.path().join("extracted_zip");

    fs::create_dir_all(src_dir.join("nested/bin")).unwrap();
    fs::write(src_dir.join("nested/bin/app.bin"), b"binary-contents-12345").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        archive tar.gz ./dist ./bundle.tar.gz
        unarchive ./bundle.tar.gz -C ./extracted_tar

        archive zip ./dist ./bundle.zip
        unarchive ./bundle.zip -C ./extracted_zip
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let tar_file = fs::File::open(tmp.path().join("bundle.tar.gz")).unwrap();
    let decoder = flate2::read::GzDecoder::new(tar_file);
    let mut tar = tar::Archive::new(decoder);
    let tar_names = tar
        .entries()
        .unwrap()
        .map(|entry| String::from_utf8_lossy(&entry.unwrap().path_bytes()).into_owned())
        .collect::<Vec<_>>();
    assert!(tar_names.iter().all(|name| !name.contains('\\')));

    let zip_file = fs::File::open(tmp.path().join("bundle.zip")).unwrap();
    let mut zip = zip::ZipArchive::new(zip_file).unwrap();
    let zip_names = (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_string())
        .collect::<Vec<_>>();
    assert!(zip_names.iter().all(|name| !name.contains('\\')));

    assert!(tar_names.iter().any(|name| name == "nested/bin/app.bin"));
    assert!(zip_names.iter().any(|name| name == "nested/bin/app.bin"));

    assert!(extract_tar.join("nested/bin/app.bin").is_file());
    assert_eq!(
        fs::read(extract_tar.join("nested/bin/app.bin")).unwrap(),
        b"binary-contents-12345"
    );

    assert!(extract_zip.join("nested/bin/app.bin").is_file());
    assert_eq!(
        fs::read(extract_zip.join("nested/bin/app.bin")).unwrap(),
        b"binary-contents-12345"
    );
}
