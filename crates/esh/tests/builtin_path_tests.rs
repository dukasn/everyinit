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
use std::path::PathBuf;
use tempfile::tempdir;

#[tokio::test]
async fn realpath_canonicalizes_operands_and_continues_after_resolution_errors() {
    let tmp = tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/file"), b"data").unwrap();
    fs::write(tmp.path().join("other"), b"more").unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("realpath sub/./file missing other", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!(
            "{}/sub/file\n{}/other\n",
            tmp.path().display(),
            tmp.path().display()
        )
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot resolve 'missing'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -q missing", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn realpath_relative_base_and_option_errors_are_explicit() {
    let tmp = tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/file"), b"data").unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("realpath --relative-to . sub/file", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "sub/file\n");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -z sub/file", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath --relative-to", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("requires a directory")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -E sub/new-file", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}/sub/new-file\n", tmp.path().display())
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -e sub/new-file", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -E missing/child", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -e -E sub/new-file", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}/sub/new-file\n", tmp.path().display())
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -E -e sub/new-file", &mut ctx)
            .await
            .unwrap(),
        1
    );
}

#[cfg(unix)]
#[tokio::test]
async fn realpath_permissive_mode_resolves_dangling_leaf_symlinks() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    symlink(
        tmp.path().join("missing-target"),
        tmp.path().join("sub/link"),
    )
    .unwrap();
    symlink(
        tmp.path().join("missing-parent/child"),
        tmp.path().join("sub/deep-link"),
    )
    .unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("realpath -E sub/link", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        ctx.get_captured_stdout().unwrap(),
        format!("{}/missing-target\n", tmp.path().display())
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -E sub/deep-link", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn realpath_existing_mode_matches_host_utility_when_supported() {
    use std::process::Command;

    let tmp = tempdir().unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub/file"), b"data").unwrap();

    let Ok(reference) = Command::new("realpath")
        .args(["-e", "sub/file"])
        .current_dir(tmp.path())
        .output()
    else {
        return;
    };
    if !reference.status.success() {
        let diagnostic = String::from_utf8_lossy(&reference.stderr);
        if diagnostic.contains("invalid option")
            || diagnostic.contains("unrecognized option")
            || diagnostic.contains("unknown option")
        {
            // Older host utilities may not implement the Issue 8 -e option.
            return;
        }
        panic!("host realpath -e failed for an existing file: {diagnostic}");
    }

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str("realpath -e sub/file", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), reference.stdout);

    let missing_reference = Command::new("realpath")
        .args(["-e", "sub/missing"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    assert!(!missing_reference.status.success());
    assert!(missing_reference.stdout.is_empty());

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("realpath -e sub/missing", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(ctx.get_captured_stdout_bytes().unwrap(), b"");
}

#[tokio::test]
async fn mkdir_rejects_unknown_options_and_supports_end_of_options() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("mkdir -- -leading", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(tmp.path().join("-leading").is_dir());

    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("mkdir -z ignored", &mut ctx).await.unwrap(),
        2
    );
    assert!(!tmp.path().join("ignored").exists());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-z'")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn mkdir_p_clears_inherited_special_bits_on_new_intermediate_directories() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let tmp = tempdir().unwrap();
    let parent = tmp.path().join("setgid-parent");
    fs::create_dir(&parent).unwrap();
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o2777)).unwrap();
    assert_eq!(fs::metadata(&parent).unwrap().mode() & 0o7000, 0o2000);

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("mkdir -p setgid-parent/first/second", &mut ctx)
            .await
            .unwrap(),
        0
    );

    let first_mode = fs::metadata(parent.join("first")).unwrap().mode() & 0o7777;
    assert_eq!(first_mode & 0o7000, 0);
}

#[tokio::test]
async fn mkdir_continues_after_operand_errors_and_accepts_grouped_parents() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::write(tmp.path().join("occupied"), "file").unwrap();
    assert_eq!(
        engine
            .eval_str("mkdir -pp occupied created nested/child", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert!(tmp.path().join("created").is_dir());
    assert!(tmp.path().join("nested/child").is_dir());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot create directory 'occupied'")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn mkdir_applies_octal_and_symbolic_modes_to_new_directories() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("mkdir -m 700 private", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );

    assert_eq!(
        engine
            .eval_str("mkdir -pm755 nested/child", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("nested/child"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o755
    );
    assert_eq!(
        fs::metadata(tmp.path().join("nested"))
            .unwrap()
            .permissions()
            .mode()
            & 0o300,
        0o300
    );

    assert_eq!(
        engine
            .eval_str("mkdir -m u=rwx,g=rx,o= private-symbolic", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("private-symbolic"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o750
    );

    assert_eq!(
        engine
            .eval_str("mkdir -m u=rw,g=u,o=g copied-symbolic", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("copied-symbolic"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o666
    );

    assert_eq!(
        engine
            .eval_str("mkdir -m u=rwxs,g=rxs,o= special-symbolic", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("special-symbolic"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o6750
    );

    assert_eq!(
        engine
            .eval_str("mkdir -m a+t sticky-symbolic", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("sticky-symbolic"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o1777
    );

    assert_eq!(
        engine
            .eval_str(
                "mkdir -m u+t user-sticky-selector; mkdir -m g+t group-sticky-selector; mkdir -m o+t other-sticky-selector",
                &mut ctx,
            )
            .await
            .unwrap(),
        0
    );
    for (path, expected_mode) in [
        ("user-sticky-selector", 0o777),
        ("group-sticky-selector", 0o777),
        ("other-sticky-selector", 0o1777),
    ] {
        assert_eq!(
            fs::metadata(tmp.path().join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o7777,
            expected_mode
        );
    }

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("mkdir -p -m 777 private", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(tmp.path().join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o7777,
        0o700
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("mkdir -m +x unsupported", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(!tmp.path().join("unsupported").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn mkdir_p_existing_directory_is_idempotent_and_does_not_apply_requested_mode() {
    use std::os::unix::fs::PermissionsExt;

    let tmp = tempdir().unwrap();
    let existing = tmp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    fs::set_permissions(&existing, fs::Permissions::from_mode(0o511)).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("mkdir -pm700 existing", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::metadata(existing).unwrap().permissions().mode() & 0o7777,
        0o511
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[cfg(unix)]
#[tokio::test]
async fn mkdir_p_traverses_existing_symlink_parent_without_replacing_it() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target");
    let link = tmp.path().join("link");
    fs::create_dir(&target).unwrap();
    symlink(&target, &link).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("mkdir -p link/child", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(target.join("child").is_dir());
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(ctx.get_captured_stderr().unwrap(), "");
}

#[tokio::test]
async fn rm_handles_options_symlinks_and_continues_after_errors() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::create_dir(tmp.path().join("target")).unwrap();
    fs::write(tmp.path().join("target/keep"), "data").unwrap();
    fs::write(tmp.path().join("-leading"), "data").unwrap();
    fs::write(tmp.path().join("later"), "data").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("target", tmp.path().join("directory-link")).unwrap();
        symlink("missing-target", tmp.path().join("dangling-link")).unwrap();
    }

    #[cfg(unix)]
    let script = "rm -- -leading directory-link dangling-link absent later";
    #[cfg(not(unix))]
    let script = "rm -- -leading absent later";
    assert_eq!(engine.eval_str(script, &mut ctx).await.unwrap(), 1);
    assert!(!tmp.path().join("-leading").exists());
    assert!(!tmp.path().join("later").exists());
    assert!(tmp.path().join("target/keep").exists());
    #[cfg(unix)]
    {
        assert!(!tmp.path().join("directory-link").exists());
        assert!(!tmp.path().join("dangling-link").exists());
    }

    ctx.enable_capture();
    assert_eq!(engine.eval_str("rm -f absent", &mut ctx).await.unwrap(), 0);
    assert_eq!(engine.eval_str("rm -f target", &mut ctx).await.unwrap(), 1);
    assert!(tmp.path().join("target").is_dir());

    ctx.enable_capture();
    assert_eq!(engine.eval_str("rm -z later", &mut ctx).await.unwrap(), 2);
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option")
    );
    assert!(tmp.path().join("target").is_dir());

    fs::write(tmp.path().join("confirm-me"), "safe").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("rm -i confirm-me", &mut ctx).await.unwrap(),
        2
    );
    assert!(tmp.path().join("confirm-me").exists());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-i'")
    );

    fs::write(tmp.path().join("marker"), "safe").unwrap();
    ctx.enable_capture();
    assert_eq!(engine.eval_str("rm -rf .", &mut ctx).await.unwrap(), 1);
    assert!(tmp.path().join("marker").exists());
}

#[cfg(unix)]
#[tokio::test]
async fn rm_force_does_not_hide_errors_after_trailing_slash_symlink_traversal() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::create_dir(tmp.path().join("target")).unwrap();
    fs::write(tmp.path().join("target/keep"), "data").unwrap();
    symlink("target", tmp.path().join("link")).unwrap();

    assert_eq!(engine.eval_str("rm -rf link/", &mut ctx).await.unwrap(), 1);
    assert!(!tmp.path().join("target/keep").exists());
    assert!(
        fs::symlink_metadata(tmp.path().join("link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot remove 'link/': Not a directory")
    );
}

#[cfg(unix)]
#[tokio::test]
async fn rm_unlinks_a_symlink_to_root_without_following_it() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    symlink("/", tmp.path().join("root-link")).unwrap();

    assert_eq!(engine.eval_str("rm root-link", &mut ctx).await.unwrap(), 0);
    assert!(fs::symlink_metadata(tmp.path().join("root-link")).is_err());
    assert!(PathBuf::from("/").is_dir());
}

#[tokio::test]
async fn touch_updates_times_without_truncating_and_handles_operands() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let file_path = tmp.path().join("existing");
    fs::write(&file_path, "preserve this content").unwrap();
    let old_time = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1);
    fs::File::options()
        .write(true)
        .open(&file_path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(old_time)
                .set_accessed(old_time),
        )
        .unwrap();

    assert_eq!(
        engine
            .eval_str("touch -m existing", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(fs::read(&file_path).unwrap(), b"preserve this content");
    assert!(fs::metadata(&file_path).unwrap().modified().unwrap() > old_time);

    fs::File::options()
        .write(true)
        .open(&file_path)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_modified(old_time)
                .set_accessed(old_time),
        )
        .unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -a existing", &mut ctx)
            .await
            .unwrap(),
        0
    );
    let metadata = fs::metadata(&file_path).unwrap();
    assert_eq!(metadata.modified().unwrap(), old_time);
    assert!(metadata.accessed().unwrap() > old_time);

    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("touch -c absent", &mut ctx).await.unwrap(),
        0
    );
    assert!(!tmp.path().join("absent").exists());

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch missing-parent/file existing", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot touch 'missing-parent/file'")
    );
    assert!(fs::metadata(&file_path).unwrap().modified().unwrap() > old_time);

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -z existing", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-z'")
    );
}

#[tokio::test]
async fn touch_copies_reference_timestamps_and_honors_access_modification_selection() {
    use chrono::{Local, TimeZone};
    use std::time::{Duration, UNIX_EPOCH};

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    let reference = tmp.path().join("reference file");
    let target = tmp.path().join("target");
    fs::write(&reference, "reference").unwrap();
    fs::write(&target, "target").unwrap();
    let reference_access = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let reference_modified = UNIX_EPOCH + Duration::from_secs(1_650_000_000);
    fs::File::open(&reference)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_accessed(reference_access)
                .set_modified(reference_modified),
        )
        .unwrap();

    assert_eq!(
        engine
            .eval_str("touch -r 'reference file' target", &mut ctx)
            .await
            .unwrap(),
        0
    );
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.accessed().unwrap(), reference_access);
    assert_eq!(metadata.modified().unwrap(), reference_modified);

    let target_access = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
    let target_modified = UNIX_EPOCH + Duration::from_secs(1_710_000_000);
    fs::File::open(&target)
        .unwrap()
        .set_times(
            fs::FileTimes::new()
                .set_accessed(target_access)
                .set_modified(target_modified),
        )
        .unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -m -rreference\\ file target", &mut ctx)
            .await
            .unwrap(),
        0
    );
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.accessed().unwrap(), target_access);
    assert_eq!(metadata.modified().unwrap(), reference_modified);

    let explicit = Local
        .with_ymd_and_hms(2020, 1, 2, 3, 4, 5)
        .single()
        .expect("unambiguous local timestamp");
    let explicit_time = UNIX_EPOCH + Duration::from_secs(explicit.timestamp() as u64);
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -at202001020304.05 target", &mut ctx)
            .await
            .unwrap(),
        0
    );
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.accessed().unwrap(), explicit_time);
    assert_eq!(metadata.modified().unwrap(), reference_modified);

    let utc_datetime =
        chrono::DateTime::parse_from_rfc3339("2020-01-02T03:04:05.123456789Z").unwrap();
    let utc_time = UNIX_EPOCH
        + Duration::new(
            utc_datetime.timestamp() as u64,
            utc_datetime.timestamp_subsec_nanos(),
        );
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -d '2020-01-02T03:04:05.123456789Z' target", &mut ctx)
            .await
            .unwrap(),
        0
    );
    let metadata = fs::metadata(&target).unwrap();
    assert_eq!(metadata.accessed().unwrap(), utc_time);
    assert_eq!(metadata.modified().unwrap(), utc_time);

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -r missing target", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("cannot access reference file 'missing'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("touch -t invalid target", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("invalid timestamp 'invalid'")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str(
                "touch -r 'reference file' -d 2020-01-02T03:04:05Z target",
                &mut ctx
            )
            .await
            .unwrap(),
        2
    );
}

#[tokio::test]
async fn touch_rejects_conflicting_timestamp_sources_in_either_order() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    for command in [
        "touch -r missing -t 202001010000 target",
        "touch -t 202001010000 -r missing target",
        "touch -r missing -d 2020-01-02T03:04:05Z target",
        "touch -d 2020-01-02T03:04:05Z -r missing target",
        "touch -t 202001010000 -d 2020-01-02T03:04:05Z target",
        "touch -d 2020-01-02T03:04:05Z -t 202001010000 target",
    ] {
        ctx.enable_capture();
        assert_eq!(
            engine.eval_str(command, &mut ctx).await.unwrap(),
            2,
            "{command}"
        );
        assert!(
            ctx.get_captured_stderr()
                .unwrap()
                .contains("are mutually exclusive"),
            "{command}"
        );
        assert!(!tmp.path().join("target").exists(), "{command}");
    }
}

#[tokio::test]
async fn mv_parses_options_moves_multiple_sources_and_continues_after_errors() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::create_dir(tmp.path().join("destination")).unwrap();
    fs::write(tmp.path().join("first"), "one").unwrap();
    fs::write(tmp.path().join("second"), "two").unwrap();
    assert_eq!(
        engine
            .eval_str("mv -f -- first second destination", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/first")).unwrap(),
        b"one"
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/second")).unwrap(),
        b"two"
    );

    fs::write(tmp.path().join("later"), "later").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("mv missing later destination", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/later")).unwrap(),
        b"later"
    );

    fs::write(tmp.path().join("-leading"), "leading").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("mv -- -leading destination/renamed", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/renamed")).unwrap(),
        b"leading"
    );

    fs::write(tmp.path().join("untouched"), "keep").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("mv -z untouched target", &mut ctx)
            .await
            .unwrap(),
        2
    );
    assert!(tmp.path().join("untouched").exists());
    assert!(
        ctx.get_captured_stderr()
            .unwrap()
            .contains("unsupported option '-z'")
    );
}

#[tokio::test]
async fn cp_parses_options_copies_multiple_sources_and_prevents_recursive_self_copy() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::create_dir(tmp.path().join("destination")).unwrap();
    fs::write(tmp.path().join("first"), "one").unwrap();
    fs::write(tmp.path().join("second"), "two").unwrap();
    assert_eq!(
        engine
            .eval_str("cp -- first second destination", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/first")).unwrap(),
        b"one"
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/second")).unwrap(),
        b"two"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            tmp.path().join("destination/first"),
            fs::Permissions::from_mode(0o444),
        )
        .unwrap();
        if fs::OpenOptions::new()
            .write(true)
            .open(tmp.path().join("destination/first"))
            .is_err()
        {
            ctx.enable_capture();
            assert_eq!(
                engine
                    .eval_str("cp first destination/first", &mut ctx)
                    .await
                    .unwrap(),
                1
            );

            ctx.enable_capture();
            assert_eq!(
                engine
                    .eval_str("cp -f first destination/first", &mut ctx)
                    .await
                    .unwrap(),
                0
            );
            assert_eq!(
                fs::read(tmp.path().join("destination/first")).unwrap(),
                b"one"
            );
        }
    }

    ctx.enable_capture();
    assert_eq!(
        engine.eval_str("cp first first", &mut ctx).await.unwrap(),
        1
    );
    assert_eq!(fs::read(tmp.path().join("first")).unwrap(), b"one");
    #[cfg(unix)]
    {
        fs::hard_link(tmp.path().join("first"), tmp.path().join("first-link")).unwrap();
        ctx.enable_capture();
        assert_eq!(
            engine
                .eval_str("cp first first-link", &mut ctx)
                .await
                .unwrap(),
            1
        );
        assert_eq!(fs::read(tmp.path().join("first")).unwrap(), b"one");
    }

    fs::write(tmp.path().join("-leading"), "leading").unwrap();
    fs::write(tmp.path().join("destination/copied"), "old contents").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -f -- -leading destination/copied", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/copied")).unwrap(),
        b"leading"
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -R source destination", &mut ctx)
            .await
            .unwrap(),
        1
    );

    fs::create_dir_all(tmp.path().join("source/nested")).unwrap();
    fs::write(tmp.path().join("source/nested/file"), "tree").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink("nested/file", tmp.path().join("source/file-link")).unwrap();
        symlink("nested", tmp.path().join("source/dir-link")).unwrap();
    }
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -R source destination", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/source/nested/file")).unwrap(),
        b"tree"
    );
    #[cfg(unix)]
    {
        assert!(
            fs::symlink_metadata(tmp.path().join("destination/source/file-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(
            fs::symlink_metadata(tmp.path().join("destination/source/dir-link"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -R source source/copy", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert!(!tmp.path().join("source/copy").exists());

    fs::write(tmp.path().join("later"), "later").unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp missing later destination", &mut ctx)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        fs::read(tmp.path().join("destination/later")).unwrap(),
        b"later"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn mv_renames_symlink_entries_without_changing_their_referents() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::write(tmp.path().join("source-target"), "source contents").unwrap();
    fs::write(
        tmp.path().join("destination-target"),
        "destination contents",
    )
    .unwrap();
    symlink("source-target", tmp.path().join("source-link")).unwrap();
    symlink("destination-target", tmp.path().join("destination-link")).unwrap();

    assert_eq!(
        engine
            .eval_str("mv source-link destination-link", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(!tmp.path().join("source-link").exists());
    assert_eq!(
        fs::read_link(tmp.path().join("destination-link")).unwrap(),
        std::path::PathBuf::from("source-target")
    );
    assert_eq!(
        fs::read(tmp.path().join("source-target")).unwrap(),
        b"source contents"
    );
    assert_eq!(
        fs::read(tmp.path().join("destination-target")).unwrap(),
        b"destination contents"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cp_follows_source_links_by_default_but_preserves_them_with_recursive_mode() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("target"), "target contents").unwrap();
    symlink("target", tmp.path().join("source-link")).unwrap();
    fs::write(tmp.path().join("destination-target"), "old contents").unwrap();
    symlink("destination-target", tmp.path().join("destination-link")).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("cp source-link copied-file", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("copied-file")).unwrap(),
        b"target contents"
    );
    assert!(
        !fs::symlink_metadata(tmp.path().join("copied-file"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -R source-link copied-link", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("copied-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_link(tmp.path().join("copied-link")).unwrap(),
        PathBuf::from("target")
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp source-link destination-link", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("destination-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read(tmp.path().join("destination-target")).unwrap(),
        b"target contents"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cp_link_modes_control_recursive_symlink_traversal_and_detect_cycles() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    fs::create_dir_all(tmp.path().join("source/real-dir")).unwrap();
    fs::write(tmp.path().join("source/file"), "contents").unwrap();
    fs::write(tmp.path().join("source/real-dir/deep"), "deep").unwrap();
    symlink("file", tmp.path().join("source/file-link")).unwrap();
    symlink("real-dir", tmp.path().join("source/dir-link")).unwrap();
    symlink("source", tmp.path().join("top-link")).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str("cp -RH top-link copied-h", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("copied-h/file-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("copied-h/dir-link"))
            .unwrap()
            .file_type()
            .is_symlink()
    );

    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -RLP top-link copied-last-p", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert!(
        fs::symlink_metadata(tmp.path().join("copied-last-p"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -RPL top-link copied-last-l", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        fs::read(tmp.path().join("copied-last-l/file-link")).unwrap(),
        b"contents"
    );
    symlink(".", tmp.path().join("source/real-dir/loop")).unwrap();
    ctx.enable_capture();
    assert_eq!(
        engine
            .eval_str("cp -RL source copied-cycle", &mut ctx)
            .await
            .unwrap(),
        1
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cp_masks_new_file_and_directory_modes_but_keeps_owner_access_during_recursive_copy() {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("source-file"), "contents").unwrap();
    fs::set_permissions(
        tmp.path().join("source-file"),
        fs::Permissions::from_mode(0o777),
    )
    .unwrap();
    let _mode_probe = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o777)
        .open(tmp.path().join("mode-probe-file"))
        .unwrap();

    fs::create_dir_all(tmp.path().join("source-tree/nested")).unwrap();
    fs::write(tmp.path().join("source-tree/nested/file"), "nested").unwrap();
    fs::set_permissions(
        tmp.path().join("source-tree"),
        fs::Permissions::from_mode(0o711),
    )
    .unwrap();
    fs::set_permissions(
        tmp.path().join("source-tree/nested"),
        fs::Permissions::from_mode(0o701),
    )
    .unwrap();
    fs::create_dir(tmp.path().join("mode-probe-dir")).unwrap();

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    assert_eq!(
        engine
            .eval_str(
                "cp source-file copied-file; cp -R source-tree copied-tree",
                &mut ctx
            )
            .await
            .unwrap(),
        0
    );

    let probe_file_mode = fs::metadata(tmp.path().join("mode-probe-file"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let copied_file_mode = fs::metadata(tmp.path().join("copied-file"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied_file_mode, probe_file_mode);

    let allowed_directory_mode = fs::metadata(tmp.path().join("mode-probe-dir"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let copied_root_mode = fs::metadata(tmp.path().join("copied-tree"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    let copied_nested_mode = fs::metadata(tmp.path().join("copied-tree/nested"))
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(copied_root_mode, 0o711 & allowed_directory_mode);
    assert_eq!(copied_nested_mode, 0o701 & allowed_directory_mode);
}

#[cfg(unix)]
#[tokio::test]
async fn cp_preserve_copies_file_and_recursive_directory_metadata() {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use std::time::{Duration, UNIX_EPOCH};

    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();

    fs::create_dir_all(tmp.path().join("tree/nested")).unwrap();
    fs::write(tmp.path().join("source"), "preserved").unwrap();
    fs::write(tmp.path().join("tree/nested/file"), "nested").unwrap();

    let accessed = UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let modified = UNIX_EPOCH + Duration::from_secs(1_650_000_000);
    for path in [
        tmp.path().join("source"),
        tmp.path().join("tree/nested/file"),
    ] {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        fs::File::open(&path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_accessed(accessed)
                    .set_modified(modified),
            )
            .unwrap();
    }
    fs::set_permissions(tmp.path().join("tree"), fs::Permissions::from_mode(0o750)).unwrap();

    assert_eq!(
        engine
            .eval_str("cp -p source copied", &mut ctx)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        engine
            .eval_str("cp -Rp tree copied-tree", &mut ctx)
            .await
            .unwrap(),
        0
    );

    let source_metadata = fs::metadata(tmp.path().join("source")).unwrap();
    let copied_metadata = fs::metadata(tmp.path().join("copied")).unwrap();
    assert_eq!(copied_metadata.permissions().mode() & 0o7777, 0o640);
    assert_eq!(copied_metadata.accessed().unwrap(), accessed);
    assert_eq!(copied_metadata.modified().unwrap(), modified);
    assert_eq!(copied_metadata.uid(), source_metadata.uid());
    assert_eq!(copied_metadata.gid(), source_metadata.gid());

    let source_dir = fs::metadata(tmp.path().join("tree")).unwrap();
    let copied_dir = fs::metadata(tmp.path().join("copied-tree")).unwrap();
    assert_eq!(copied_dir.permissions().mode() & 0o7777, 0o750);
    assert_eq!(
        copied_dir.modified().unwrap(),
        source_dir.modified().unwrap()
    );
    let copied_nested = fs::metadata(tmp.path().join("copied-tree/nested/file")).unwrap();
    assert_eq!(copied_nested.permissions().mode() & 0o7777, 0o640);
    assert_eq!(copied_nested.accessed().unwrap(), accessed);
    assert_eq!(copied_nested.modified().unwrap(), modified);
}

#[cfg(unix)]
#[tokio::test]
async fn cp_preserve_keeps_recursive_symlink_timestamps() {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::symlink;
    use std::time::{Duration, UNIX_EPOCH};

    let tmp = tempdir().unwrap();
    let source = tmp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("target"), "target").unwrap();
    let source_link = source.join("link");
    symlink("target", &source_link).unwrap();

    let source_link_c = CString::new(source_link.as_os_str().as_bytes()).unwrap();
    let timestamp = libc::timespec {
        tv_sec: 1_600_000_000,
        tv_nsec: 123_456_789,
    };
    let times = [timestamp, timestamp];
    // SAFETY: source_link_c is NUL-terminated and times contains two valid
    // timestamp values for the duration of the call.
    assert_eq!(
        unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                source_link_c.as_ptr(),
                times.as_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        },
        0
    );

    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();
    ctx.set_cwd(tmp.path()).unwrap();
    assert_eq!(
        engine
            .eval_str("cp -RPp source copied", &mut ctx)
            .await
            .unwrap(),
        0
    );

    let copied_link = fs::symlink_metadata(tmp.path().join("copied/link")).unwrap();
    assert!(copied_link.file_type().is_symlink());
    assert_eq!(
        copied_link.modified().unwrap(),
        UNIX_EPOCH + Duration::from_secs(1_600_000_000) + Duration::from_nanos(123_456_789)
    );
}

#[tokio::test]
async fn test_path_dirname_and_basename() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = r#"
        path -d /foo/bar/baz.txt
        path -b /foo/bar/baz.txt
        path -e /foo/bar/baz.txt
        path -s /foo/bar/baz.txt
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines[0], "/foo/bar");
    assert_eq!(lines[1], "baz.txt");
    assert_eq!(lines[2], "txt");
    assert_eq!(lines[3], "baz");
}

#[tokio::test]
async fn test_path_join() {
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.enable_capture();

    let script = "path -j src compiler mod.rs";
    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert_eq!(output.trim(), "src/compiler/mod.rs");
}

#[tokio::test]
async fn test_path_exists_and_checks() {
    let tmp = tempdir().unwrap();
    let engine = Engine::new();
    let mut ctx = ExecutionContext::new();
    ctx.set_cwd(tmp.path()).unwrap();
    ctx.enable_capture();

    let script = r#"
        mkdir -p test_dir
        touch test_dir/test_file.txt

        path --exists test_dir && echo "dir exists"
        path --is-dir test_dir && echo "is a dir"
        path --is-file test_dir/test_file.txt && echo "is a file"
        path --exists nonexistent_item || echo "not exists"
    "#;

    let status = engine
        .eval_str(script, &mut ctx)
        .await
        .expect("eval failed");
    assert_eq!(status, 0);

    let output = ctx.get_captured_stdout().unwrap();
    assert!(output.contains("dir exists"));
    assert!(output.contains("is a dir"));
    assert!(output.contains("is a file"));
    assert!(output.contains("not exists"));
}
