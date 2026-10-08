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

use std::time::Duration;

use evin_process::{ProcessHandle, ProcessMode, ProcessSpec, ProcessTermination};
use tokio_util::sync::CancellationToken;

mod common;
use common::fixture_binary;

#[tokio::test]
#[ignore = "scheduled process-supervision soak suite"]
async fn repeated_spawn_output_and_cancellation() {
    for iteration in 0..100 {
        let args = if iteration % 2 == 0 {
            vec!["burst".to_string(), "64".to_string()]
        } else {
            vec!["sleep".to_string(), "30000".to_string()]
        };
        let spec = ProcessSpec::new(
            fixture_binary(),
            std::env::current_dir().unwrap(),
            ProcessMode::SupervisedAutomation,
        )
        .args(args);
        let token = CancellationToken::new();
        if iteration % 2 == 1 {
            token.cancel();
        }
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            ProcessHandle::spawn(spec, None).unwrap().wait(token),
        )
        .await
        .unwrap_or_else(|_| panic!("process soak iteration {iteration} timed out"))
        .unwrap();
        if iteration % 2 == 0 {
            assert!(
                matches!(result.termination, ProcessTermination::Exited(status) if status.success())
            );
        } else {
            assert!(matches!(result.termination, ProcessTermination::Cancelled));
        }
    }
}
