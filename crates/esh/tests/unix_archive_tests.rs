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
async fn test_unix_tar_czf_and_xzf() {
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("dist");
    let extract_dir = tmp.path().join("extracted");

    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("version.txt"), b"v1.0.0\n").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        tar -czf app.tar.gz ./dist
        tar -xzf app.tar.gz -C ./extracted
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let archive_file = fs::File::open(tmp.path().join("app.tar.gz")).unwrap();
    let decoder = flate2::read::GzDecoder::new(archive_file);
    let mut archive = tar::Archive::new(decoder);
    let names = archive
        .entries()
        .unwrap()
        .map(|entry| String::from_utf8_lossy(&entry.unwrap().path_bytes()).into_owned())
        .collect::<Vec<_>>();
    assert!(names.iter().all(|name| !name.contains('\\')));
    assert!(names.iter().any(|name| name == "dist/version.txt"));

    let extracted_file = extract_dir.join("dist").join("version.txt");
    assert!(extracted_file.is_file() || extract_dir.join("version.txt").is_file());
}

#[tokio::test]
async fn test_unix_tar_extracts_concatenated_archives_from_stdin() {
    let tmp = tempdir().unwrap();
    let first = tmp.path().join("first.tar");
    let second = tmp.path().join("second.tar");
    let extract_dir = tmp.path().join("extracted");

    for (archive, name, contents) in [
        (&first, "first.txt", b"first".as_slice()),
        (&second, "second.txt", b"second".as_slice()),
    ] {
        let file = fs::File::create(archive).unwrap();
        let mut builder = tar::Builder::new(file);
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, name, contents).unwrap();
        builder.finish().unwrap();
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let code = engine
        .eval_str(
            "cat first.tar second.tar | tar --ignore-zeros -x -C ./extracted",
            &mut ctx,
        )
        .await
        .unwrap();
    assert_eq!(code, 0);
    assert_eq!(fs::read(extract_dir.join("first.txt")).unwrap(), b"first");
    assert_eq!(fs::read(extract_dir.join("second.txt")).unwrap(), b"second");
}

#[tokio::test]
async fn test_unix_zip_and_unzip() {
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("dist");
    let extract_dir = tmp.path().join("extracted_zip");

    fs::create_dir_all(src_dir.join("nested/data")).unwrap();
    fs::write(src_dir.join("nested/data/meta.json"), b"{\"ok\":true}").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    let _ = ctx.set_cwd(tmp.path());

    let script = r#"
        zip -r app.zip ./dist
        unzip app.zip -d ./extracted_zip
    "#;

    let code = engine.eval_str(script, &mut ctx).await.unwrap();
    assert_eq!(code, 0);

    let archive_file = fs::File::open(tmp.path().join("app.zip")).unwrap();
    let mut archive = zip::ZipArchive::new(archive_file).unwrap();
    let names = (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .unwrap()
                .name()
                .unwrap()
                .into_owned()
        })
        .collect::<Vec<_>>();
    assert!(names.iter().all(|name| !name.contains('\\')));
    assert!(names.iter().any(|name| name == "nested/data/meta.json"));

    assert!(
        extract_dir.join("dist/nested/data/meta.json").is_file()
            || extract_dir.join("nested/data/meta.json").is_file()
    );
}
