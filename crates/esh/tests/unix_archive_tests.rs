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

    let extracted_file = extract_dir.join("dist").join("version.txt");
    assert!(extracted_file.is_file() || extract_dir.join("version.txt").is_file());
}

#[tokio::test]
async fn test_unix_zip_and_unzip() {
    let tmp = tempdir().unwrap();
    let src_dir = tmp.path().join("dist");
    let extract_dir = tmp.path().join("extracted_zip");

    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("meta.json"), b"{\"ok\":true}").unwrap();

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

    assert!(
        extract_dir.join("dist").join("meta.json").is_file()
            || extract_dir.join("meta.json").is_file()
    );
}
