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
async fn crypto_verify_accepts_the_rfc8032_ed25519_vector() {
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join("key.pem"),
        "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA11qYAYKxCrfVS/7TyWQHOg7hcvPapiMlrwIaaPcHURo=\n-----END PUBLIC KEY-----\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("signature"),
        [
            0xe5, 0x56, 0x43, 0x00, 0xc3, 0x60, 0xac, 0x72, 0x90, 0x86, 0xe2, 0xcc, 0x80, 0x6e,
            0x82, 0x8a, 0x84, 0x87, 0x7f, 0x1e, 0xb8, 0xe5, 0xd9, 0x74, 0xd8, 0x73, 0xe0, 0x65,
            0x22, 0x49, 0x01, 0x55, 0x5f, 0xb8, 0x82, 0x15, 0x90, 0xa3, 0x3b, 0xac, 0xc6, 0x1e,
            0x39, 0x70, 0x1c, 0xf9, 0xb4, 0x6b, 0xd2, 0x5b, 0xf5, 0xf0, 0x59, 0x5b, 0xbe, 0x24,
            0x65, 0x51, 0x41, 0x43, 0x8e, 0x7a, 0x10, 0x0b,
        ],
    )
    .unwrap();
    fs::write(tmp.path().join("message"), []).unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str(
                "crypto --verify --pubkey key.pem --sig signature message",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );

    let mut legacy_ctx = ExecutionContext::new();
    legacy_ctx.set_cwd(tmp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str(
                "crypto verify --pubkey key.pem --sig signature message",
                &mut legacy_ctx,
            )
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn crypto_generates_signs_and_exports_ed25519_keys() {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("message"), "signed content\n").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "crypto --generate --private private.pem --public public.pem\ncrypto --sign --key private.pem message > signature\ncrypto --public-key --key private.pem > exported.pem\ncrypto --verify --pubkey public.pem --sig signature message\ncrypto --verify --pubkey exported.pem --sig signature message",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    assert!(
        fs::read_to_string(tmp.path().join("private.pem"))
            .unwrap()
            .starts_with("-----BEGIN PRIVATE KEY-----\n")
    );
    assert_eq!(fs::read(tmp.path().join("signature")).unwrap().len(), 64);
}

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
