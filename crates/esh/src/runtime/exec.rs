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

use crate::error::ExecError;
use crate::parser::ast::RedirectionOp;
use crate::planner::compiler::compile_script;
use crate::planner::dispatch::resolve_builtin;
use crate::planner::ir::{
    CommandTarget, ExecutionPlan, ExecutionStage, PlanNode, PlanRedirection, PlanWord, PlanWordPart,
};
use crate::runtime::context::{ExecutionContext, TrapSignal};
use crate::runtime::streams::StreamTarget;
use crate::runtime::traits::{BoxFuture, Executable, ScopeManager};
use evin_process::{
    EnvironmentPolicy, OutputChunk, OutputPolicy, ProcessError, ProcessHandle, ProcessMode,
    ProcessSpec, ProcessTermination, StdinPolicy,
};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::task::JoinSet;

async fn execute_interrupt_trap(ctx: &mut ExecutionContext) {
    if ctx.is_in_trap() || !ctx.take_interrupt_trap() {
        return;
    }
    let Some(handler) = ctx.get_trap(TrapSignal::Interrupt) else {
        return;
    };

    ctx.set_in_trap(true);
    ctx.push_scope();
    let cancelled_token = ctx.replace_token(tokio_util::sync::CancellationToken::new());
    let _ = handler.execute(ctx).await;
    ctx.replace_token(cancelled_token);
    let _ = ctx.pop_scope();
    ctx.set_in_trap(false);
}

async fn execute_exit_trap(ctx: &mut ExecutionContext) {
    if !ctx.is_root_exec() || ctx.is_in_trap() {
        return;
    }
    let Some(handler) = ctx.get_trap(TrapSignal::Exit) else {
        return;
    };

    // The handler runs as shell code even when an `exit` or `errexit` request
    // is pending. Preserve that request while allowing the handler's own plan
    // to execute normally.
    let pending_exit = ctx.take_exit_request();
    ctx.set_in_trap(true);
    ctx.push_scope();
    let previous_token = ctx.replace_token(tokio_util::sync::CancellationToken::new());
    let _ = handler.execute(ctx).await;
    ctx.replace_token(previous_token);
    let _ = ctx.pop_scope();
    ctx.set_in_trap(false);
    if ctx.exit_code().is_none()
        && let Some(code) = pending_exit
    {
        ctx.restore_exit_request(code);
    }
}

#[derive(Clone, PartialEq, Eq)]
enum OutputDestination {
    ContextStdout,
    ContextStderr,
    Target(usize, StreamTarget),
}

fn destination_from_target(target: StreamTarget, identity: usize) -> OutputDestination {
    match target {
        StreamTarget::Stdout => OutputDestination::ContextStdout,
        StreamTarget::Stderr => OutputDestination::ContextStderr,
        other => OutputDestination::Target(identity, other),
    }
}

fn output_destinations(
    redirections: &[PlanRedirection],
    targets: &[Option<StreamTarget>],
) -> (OutputDestination, OutputDestination) {
    let mut stdout = OutputDestination::ContextStdout;
    let mut stderr = OutputDestination::ContextStderr;

    for (index, redirection) in redirections.iter().enumerate() {
        match redirection.op {
            RedirectionOp::RedirectStderrToStdout => stderr = stdout.clone(),
            RedirectionOp::RedirectStdoutToStderr => stdout = stderr.clone(),
            RedirectionOp::RedirectOut
            | RedirectionOp::RedirectClobber
            | RedirectionOp::RedirectAppend => {
                stdout = destination_from_target(
                    targets[index]
                        .clone()
                        .expect("output target prepared before command execution"),
                    index,
                );
            }
            RedirectionOp::RedirectStderr | RedirectionOp::RedirectStderrAppend => {
                stderr = destination_from_target(
                    targets[index]
                        .clone()
                        .expect("output target prepared before command execution"),
                    index,
                );
            }
            RedirectionOp::RedirectAll | RedirectionOp::RedirectAllAppend => {
                let destination = destination_from_target(
                    targets[index]
                        .clone()
                        .expect("output target prepared before command execution"),
                    index,
                );
                stdout = destination.clone();
                stderr = destination;
            }
            RedirectionOp::RedirectIn
            | RedirectionOp::RedirectHereString
            | RedirectionOp::RedirectHereDoc
            | RedirectionOp::RedirectHereDocStrip => {}
        }
    }
    (stdout, stderr)
}

fn distinct_file_destinations(
    stdout: &OutputDestination,
    stderr: &OutputDestination,
) -> Option<((std::path::PathBuf, bool), (std::path::PathBuf, bool))> {
    let (
        OutputDestination::Target(
            stdout_identity,
            StreamTarget::File {
                path: stdout_path,
                append: stdout_append,
            },
        ),
        OutputDestination::Target(
            stderr_identity,
            StreamTarget::File {
                path: stderr_path,
                append: stderr_append,
            },
        ),
    ) = (stdout, stderr)
    else {
        return None;
    };
    if stdout_identity == stderr_identity {
        return None;
    }
    Some((
        (stdout_path.clone(), *stdout_append),
        (stderr_path.clone(), *stderr_append),
    ))
}

fn write_to_destination(
    destination: &OutputDestination,
    bytes: &[u8],
    ctx: &mut ExecutionContext,
) -> Result<(), ExecError> {
    match destination {
        OutputDestination::ContextStdout => ctx.write_stdout(bytes),
        OutputDestination::ContextStderr => ctx.write_stderr(bytes),
        OutputDestination::Target(_, target) => target.write_data(bytes)?,
    }
    Ok(())
}

async fn report_command_failure(
    ctx: &mut ExecutionContext,
    command_name: &str,
    line: u32,
    code: i32,
    message: Option<String>,
) {
    ctx.record_failure(command_name, line, code, message);
    if !ctx.is_in_trap()
        && let Some(err_trap) = ctx.get_trap(TrapSignal::Err)
    {
        ctx.set_in_trap(true);
        ctx.push_scope();
        let _ = err_trap.execute(ctx).await;
        let _ = ctx.pop_scope();
        ctx.set_in_trap(false);
    }
}

async fn redirection_failure(
    error: ExecError,
    command_name: &str,
    line: u32,
    stderr_destination: &OutputDestination,
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    let message = error.to_string();
    let diagnostic = format!("esh: {message}\n");
    if write_to_destination(stderr_destination, diagnostic.as_bytes(), ctx).is_err() {
        ctx.write_stderr(diagnostic.as_bytes());
    }
    let code = error.exit_code();
    report_command_failure(ctx, command_name, line, code, Some(message)).await;
    Ok(code)
}

fn external_process_exit_code(status: std::process::ExitStatus) -> i32 {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }

    #[cfg(windows)]
    if status.code() == Some(0xC000_013A_u32 as i32) {
        return 130;
    }

    status.code().unwrap_or(1)
}

impl Executable for ExecutionPlan {
    fn execute<'a>(
        &'a self,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, ExecError>> {
        Box::pin(async move {
            ctx.enter_exec();
            let mut last_status = 0;
            for stage in &self.stages {
                if ctx.is_exiting()
                    || ctx.is_returning()
                    || ctx.break_requested().is_some()
                    || ctx.continue_requested().is_some()
                {
                    break;
                }

                if ctx.is_cancelled() {
                    ctx.record_failure(
                        "interrupted",
                        0,
                        130,
                        Some("Execution cancelled".to_string()),
                    );
                    execute_interrupt_trap(ctx).await;
                    ctx.leave_exec();
                    execute_exit_trap(ctx).await;
                    return Ok(130);
                }

                last_status = match stage.execute(ctx).await {
                    Ok(status) => status,
                    Err(error) => {
                        if ctx.is_cancelled() {
                            execute_interrupt_trap(ctx).await;
                        }
                        ctx.set_last_exit(error.exit_code());
                        ctx.leave_exec();
                        execute_exit_trap(ctx).await;
                        return Err(error);
                    }
                };
                // Some builtins observe cancellation themselves and return an
                // interrupted status before the stage-level select does.
                if ctx.is_cancelled() {
                    execute_interrupt_trap(ctx).await;
                }
                ctx.set_last_exit(last_status);

                if last_status != 0 {
                    if ctx.options().errexit
                        && !ctx.is_errexit_shielded()
                        && !ctx.fail_fast_enabled()
                    {
                        ctx.request_exit(last_status);
                        break;
                    }

                    if ctx.fail_fast_enabled() {
                        break;
                    }
                }
            }

            ctx.leave_exec();

            execute_exit_trap(ctx).await;

            Ok(ctx.exit_code().unwrap_or(last_status))
        })
    }
}

impl Executable for ExecutionStage {
    fn execute<'a>(
        &'a self,
        ctx: &'a mut ExecutionContext,
    ) -> BoxFuture<'a, Result<i32, ExecError>> {
        Box::pin(async move {
            if ctx.is_cancelled() {
                ctx.record_failure(
                    "interrupted",
                    0,
                    130,
                    Some("Execution cancelled".to_string()),
                );
                execute_interrupt_trap(ctx).await;
                return Ok(130);
            }

            let token = ctx.token().clone();

            tokio::select! {
                biased;
                res = self.execute_inner(ctx) => res,
                _ = token.cancelled() => {
                    ctx.record_failure("interrupted", 0, 130, Some("Execution cancelled".to_string()));
                    execute_interrupt_trap(ctx).await;
                    Ok(130)
                }
            }
        })
    }
}

impl ExecutionStage {
    async fn execute_inner(&self, ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
        match self {
            ExecutionStage::Pipeline(nodes) => execute_pipeline(nodes, ctx).await,
            ExecutionStage::PipelineStages(stages) => execute_compound_pipeline(stages, ctx).await,

            ExecutionStage::Parallel(branches) => {
                let mut join_set = JoinSet::new();
                let mut results = std::iter::repeat_with(|| None)
                    .take(branches.len())
                    .collect::<Vec<Option<Result<i32, ExecError>>>>();
                let mut outputs = std::iter::repeat_with(|| None)
                    .take(branches.len())
                    .collect::<Vec<Option<(Vec<u8>, Vec<u8>)>>>();
                let mut failure_contexts = std::iter::repeat_with(|| None)
                    .take(branches.len())
                    .collect::<Vec<Option<(String, u32, Option<String>)>>>();

                for (index, branch) in branches.iter().enumerate() {
                    let mut branch_ctx = ctx.clone_isolated();
                    branch_ctx.enable_capture();
                    let branch_stage = branch.clone();

                    join_set.spawn(async move {
                        let res = execute_isolated_stage(&branch_stage, &mut branch_ctx).await;
                        (index, res, branch_ctx)
                    });
                }

                while let Some(join_res) = join_set.join_next().await {
                    let (index, res, child_ctx) =
                        join_res.map_err(|e| ExecError::JoinError(e.to_string()))?;
                    results[index] =
                        Some(res.map(|status| child_ctx.exit_code().unwrap_or(status)));
                    outputs[index] = Some((
                        child_ctx.get_captured_stdout_bytes().unwrap_or_default(),
                        child_ctx.get_captured_stderr_bytes().unwrap_or_default(),
                    ));
                    failure_contexts[index] = child_ctx.failure_context();
                }

                for (stdout, stderr) in outputs.into_iter().flatten() {
                    ctx.write_stdout(&stdout);
                    ctx.write_stderr(&stderr);
                }

                for (index, result) in results.iter().enumerate() {
                    let failure_code = match result {
                        Some(Ok(code)) if *code != 0 => Some(*code),
                        Some(Err(error)) => Some(error.exit_code()),
                        _ => None,
                    };
                    if let Some(code) = failure_code {
                        if let Some((command, line, message)) = &failure_contexts[index] {
                            ctx.record_failure(command, *line, code, message.clone());
                        }
                        break;
                    }
                }

                let mut first_error_code = 0;
                for result in results.into_iter().flatten() {
                    let exit_code = result?;
                    if exit_code != 0 && first_error_code == 0 {
                        first_error_code = exit_code;
                    }
                }

                ctx.set_last_exit(first_error_code);
                Ok(first_error_code)
            }

            ExecutionStage::Sequential {
                left,
                require_success,
                right,
            } => {
                let prev_shield = ctx.is_errexit_shielded();
                ctx.set_errexit_shielded(true);
                let left_result = left.execute(ctx).await;
                ctx.set_errexit_shielded(prev_shield);
                let left_code = left_result?;
                ctx.set_last_exit(left_code);

                if ctx.is_exiting()
                    || ctx.is_returning()
                    || ctx.break_requested().is_some()
                    || ctx.continue_requested().is_some()
                {
                    return Ok(left_code);
                }

                if (*require_success && left_code == 0) || (!*require_success && left_code != 0) {
                    let right_code = right.execute(ctx).await?;
                    ctx.set_last_exit(right_code);
                    if right_code != 0
                        && ctx.options().errexit
                        && !ctx.is_errexit_shielded()
                        && !ctx.fail_fast_enabled()
                    {
                        ctx.request_exit(right_code);
                    }
                    Ok(right_code)
                } else {
                    Ok(left_code)
                }
            }

            ExecutionStage::FailFastBlock {
                try_stage,
                fallback,
            } => {
                let prev_fail_fast = ctx.fail_fast_enabled();
                ctx.set_fail_fast(true);

                let try_res = try_stage.execute(ctx).await;
                ctx.set_fail_fast(prev_fail_fast);

                match try_res {
                    Ok(0) => {
                        ctx.set_last_exit(0);
                        Ok(0)
                    }
                    Ok(code) => {
                        if ctx.is_exiting()
                            || ctx.is_returning()
                            || ctx.break_requested().is_some()
                            || ctx.continue_requested().is_some()
                        {
                            ctx.set_last_exit(code);
                            return Ok(code);
                        }

                        if ctx.get_var("ESH_FAILED_CMD").is_none() {
                            ctx.record_failure("block", 0, code, None);
                        } else {
                            ctx.set_last_exit(code);
                        }
                        if let Some(catch_stage) = fallback {
                            catch_stage.execute(ctx).await
                        } else {
                            Ok(code)
                        }
                    }
                    Err(e) => {
                        let code = e.exit_code();
                        if ctx.get_var("ESH_FAILED_CMD").is_none() {
                            ctx.record_failure("block", 0, code, Some(e.to_string()));
                        } else {
                            ctx.set_last_exit(code);
                        }
                        if let Some(catch_stage) = fallback {
                            catch_stage.execute(ctx).await
                        } else {
                            Err(e)
                        }
                    }
                }
            }

            ExecutionStage::Block(stages) => {
                let mut last_code = 0;
                for stage in stages {
                    if ctx.is_exiting()
                        || ctx.is_returning()
                        || ctx.break_requested().is_some()
                        || ctx.continue_requested().is_some()
                    {
                        break;
                    }
                    last_code = stage.execute(ctx).await?;
                    ctx.set_last_exit(last_code);
                    if last_code != 0
                        && ctx.options().errexit
                        && !ctx.is_errexit_shielded()
                        && !ctx.fail_fast_enabled()
                    {
                        ctx.request_exit(last_code);
                        break;
                    }
                    if last_code != 0 && ctx.fail_fast_enabled() {
                        break;
                    }
                }
                Ok(last_code)
            }

            ExecutionStage::Subshell(stages) => {
                let mut child_ctx = ctx.clone_isolated();
                child_ctx.push_scope();
                let mut last_code = 0;
                let mut execution_error = None;
                for stage in stages {
                    if child_ctx.is_exiting()
                        || child_ctx.is_returning()
                        || child_ctx.break_requested().is_some()
                        || child_ctx.continue_requested().is_some()
                        || child_ctx.is_cancelled()
                    {
                        break;
                    }

                    match stage.execute(&mut child_ctx).await {
                        Ok(status) => last_code = status,
                        Err(error) => {
                            execution_error = Some(error);
                            break;
                        }
                    }
                    child_ctx.set_last_exit(last_code);
                    if last_code != 0 && child_ctx.fail_fast_enabled() {
                        break;
                    }
                }
                child_ctx.set_last_exit(child_ctx.exit_code().unwrap_or(last_code));
                execute_exit_trap(&mut child_ctx).await;
                if let Some(stdout) = child_ctx.get_captured_stdout_bytes() {
                    ctx.write_stdout(&stdout);
                }
                if let Some(stderr) = child_ctx.get_captured_stderr_bytes() {
                    ctx.write_stderr(&stderr);
                }
                let _ = child_ctx.pop_scope();
                if let Some(error) = execution_error {
                    return Err(error);
                }
                let subshell_status = child_ctx.exit_code().unwrap_or(last_code);
                ctx.set_last_exit(subshell_status);
                Ok(subshell_status)
            }

            ExecutionStage::Negation(inner) => {
                let prev_shield = ctx.is_errexit_shielded();
                ctx.set_errexit_shielded(true);
                let inner_result = inner.execute(ctx).await;
                ctx.set_errexit_shielded(prev_shield);
                let code = inner_result?;
                let inverted = if code == 0 { 1 } else { 0 };
                ctx.set_last_exit(inverted);
                Ok(inverted)
            }

            ExecutionStage::Redirected {
                inner,
                redirections,
            } => execute_redirected_compound(inner, redirections, ctx).await,

            ExecutionStage::FunctionDef { name, plan } => {
                ctx.register_function(name.clone(), plan.clone());
                Ok(0)
            }

            ExecutionStage::If {
                condition,
                then_branch,
                elif_branches,
                else_branch,
            } => {
                let prev_shield = ctx.is_errexit_shielded();
                ctx.set_errexit_shielded(true);
                let condition_result = condition.execute(ctx).await;
                ctx.set_errexit_shielded(prev_shield);
                let cond_status = condition_result?;

                if ctx.is_cancelled() {
                    ctx.set_last_exit(130);
                    return Ok(130);
                }

                if ctx.is_exiting()
                    || ctx.is_returning()
                    || ctx.break_requested().is_some()
                    || ctx.continue_requested().is_some()
                {
                    ctx.set_last_exit(cond_status);
                    return Ok(cond_status);
                }

                if cond_status == 0 {
                    let res = then_branch.execute(ctx).await?;
                    ctx.set_last_exit(res);
                    return Ok(res);
                }

                for (elif_cond, elif_body) in elif_branches {
                    let prev_s = ctx.is_errexit_shielded();
                    ctx.set_errexit_shielded(true);
                    let elif_result = elif_cond.execute(ctx).await;
                    ctx.set_errexit_shielded(prev_s);
                    let elif_status = elif_result?;

                    if ctx.is_cancelled() {
                        ctx.set_last_exit(130);
                        return Ok(130);
                    }

                    if ctx.is_exiting()
                        || ctx.is_returning()
                        || ctx.break_requested().is_some()
                        || ctx.continue_requested().is_some()
                    {
                        ctx.set_last_exit(elif_status);
                        return Ok(elif_status);
                    }

                    if elif_status == 0 {
                        let res = elif_body.execute(ctx).await?;
                        ctx.set_last_exit(res);
                        return Ok(res);
                    }
                }

                if let Some(else_b) = else_branch {
                    let res = else_b.execute(ctx).await?;
                    ctx.set_last_exit(res);
                    return Ok(res);
                }

                ctx.set_last_exit(0);
                Ok(0)
            }

            ExecutionStage::ForLoop { var, words, body } => {
                ctx.enter_loop();
                let mut last_status = 0;

                let items_result: Result<Vec<String>, ExecError> = async {
                    if let Some(w_list) = words {
                        let mut expanded = Vec::new();
                        for w in w_list {
                            if let Some(items) = expand_word_as_items(w, ctx).await? {
                                expanded.extend(items);
                                continue;
                            }
                            let val = expand_word(w, ctx).await?;
                            if matches!(w.parts.as_slice(), [PlanWordPart::CommandSub(_)]) {
                                if !val.is_empty() {
                                    expanded.extend(val.split('\n').map(str::to_owned));
                                }
                            } else {
                                expanded.push(val);
                            }
                        }
                        Ok(expanded)
                    } else {
                        Ok(ctx.positional_args().to_vec())
                    }
                }
                .await;
                let items = match items_result {
                    Ok(items) => items,
                    Err(error) => {
                        ctx.exit_loop();
                        return Err(error);
                    }
                };
                if ctx.is_cancelled() {
                    ctx.exit_loop();
                    ctx.set_last_exit(130);
                    return Ok(130);
                }

                for item in items {
                    ctx.set_var(var, &item);

                    last_status = match body.execute(ctx).await {
                        Ok(status) => status,
                        Err(error) => {
                            ctx.exit_loop();
                            return Err(error);
                        }
                    };
                    ctx.set_last_exit(last_status);

                    if ctx.is_cancelled() {
                        last_status = 130;
                        break;
                    }

                    if ctx.is_exiting() || ctx.is_returning() {
                        break;
                    }

                    if ctx.break_requested().is_some() {
                        ctx.decrement_break();
                        break;
                    }

                    if ctx.continue_requested().is_some() {
                        let cont_depth = ctx.continue_requested().unwrap();
                        ctx.decrement_continue();
                        if cont_depth > 1 {
                            break;
                        }
                        continue;
                    }
                }

                ctx.exit_loop();
                Ok(last_status)
            }

            ExecutionStage::SelectLoop { var, words, body } => {
                ctx.enter_loop();
                let items_result: Result<Vec<String>, ExecError> = async {
                    if let Some(word_list) = words {
                        let mut items = Vec::new();
                        for word in word_list {
                            if let Some(expanded_items) = expand_word_as_items(word, ctx).await? {
                                items.extend(expanded_items);
                            } else {
                                let value = expand_word(word, ctx).await?;
                                if matches!(word.parts.as_slice(), [PlanWordPart::CommandSub(_)]) {
                                    if !value.is_empty() {
                                        items.extend(value.split('\n').map(str::to_owned));
                                    }
                                } else {
                                    items.push(value);
                                }
                            }
                        }
                        Ok(items)
                    } else {
                        Ok(ctx.positional_args().to_vec())
                    }
                }
                .await;
                let items = match items_result {
                    Ok(items) => items,
                    Err(error) => {
                        ctx.exit_loop();
                        return Err(error);
                    }
                };
                if ctx.is_cancelled() {
                    ctx.exit_loop();
                    ctx.set_last_exit(130);
                    return Ok(130);
                }
                if items.is_empty() {
                    ctx.exit_loop();
                    return Ok(0);
                }
                let mut last_status = 0;
                let mut menu_displayed = false;

                loop {
                    if ctx.is_exiting() || ctx.is_returning() || ctx.is_cancelled() {
                        if ctx.is_cancelled() {
                            last_status = 130;
                        }
                        break;
                    }
                    if !menu_displayed {
                        for (index, item) in items.iter().enumerate() {
                            ctx.write_stderr(format!("{}) {}\n", index + 1, item).as_bytes());
                        }
                        menu_displayed = true;
                    }
                    let prompt = ctx.get_var("PS3").unwrap_or_else(|| "#? ".to_string());
                    let read_args = vec!["-r".to_string(), "-p".to_string(), prompt.clone()];
                    let read_status =
                        match crate::runtime::builtins::flow::read(&read_args, ctx).await {
                            Ok(status) => status,
                            Err(error) => {
                                ctx.exit_loop();
                                return Err(error);
                            }
                        };
                    if ctx.is_cancelled() {
                        last_status = 130;
                        break;
                    }
                    if read_status != 0 {
                        last_status = read_status;
                        ctx.write_stdout(b"\n");
                        break;
                    }
                    let reply = ctx.get_var("REPLY").unwrap_or_default();
                    let selected = reply
                        .trim()
                        .strip_prefix('+')
                        .unwrap_or(reply.trim())
                        .parse::<usize>()
                        .ok()
                        .filter(|index| *index > 0 && *index <= items.len())
                        .map(|index| items[index - 1].clone())
                        .unwrap_or_default();
                    ctx.set_var(var, &selected);

                    last_status = match body.execute(ctx).await {
                        Ok(status) => status,
                        Err(error) => {
                            ctx.exit_loop();
                            return Err(error);
                        }
                    };
                    ctx.set_last_exit(last_status);
                    if ctx.is_exiting() || ctx.is_returning() || ctx.is_cancelled() {
                        break;
                    }
                    if ctx.break_requested().is_some() {
                        ctx.decrement_break();
                        break;
                    }
                    if ctx.continue_requested().is_some() {
                        let depth = ctx.continue_requested().unwrap();
                        ctx.decrement_continue();
                        if depth > 1 {
                            break;
                        }
                    }
                }

                ctx.exit_loop();
                Ok(last_status)
            }

            ExecutionStage::WhileLoop {
                condition,
                body,
                is_until,
            } => {
                ctx.enter_loop();
                let mut last_status = 0;

                loop {
                    if ctx.is_exiting() || ctx.is_returning() || ctx.is_cancelled() {
                        break;
                    }

                    let prev_shield = ctx.is_errexit_shielded();
                    ctx.set_errexit_shielded(true);
                    let condition_result = condition.execute(ctx).await;
                    ctx.set_errexit_shielded(prev_shield);
                    let cond_status = match condition_result {
                        Ok(status) => status,
                        Err(error) => {
                            ctx.exit_loop();
                            return Err(error);
                        }
                    };

                    if ctx.is_cancelled() {
                        last_status = 130;
                        break;
                    }

                    if ctx.is_exiting() || ctx.is_returning() {
                        break;
                    }

                    if ctx.break_requested().is_some() {
                        ctx.decrement_break();
                        break;
                    }

                    if ctx.continue_requested().is_some() {
                        let continue_depth = ctx.continue_requested().unwrap();
                        ctx.decrement_continue();
                        if continue_depth > 1 {
                            break;
                        }
                        continue;
                    }

                    let should_run = if *is_until {
                        cond_status != 0
                    } else {
                        cond_status == 0
                    };

                    if !should_run {
                        break;
                    }

                    last_status = match body.execute(ctx).await {
                        Ok(status) => status,
                        Err(error) => {
                            ctx.exit_loop();
                            return Err(error);
                        }
                    };
                    ctx.set_last_exit(last_status);

                    if ctx.is_exiting() || ctx.is_returning() || ctx.is_cancelled() {
                        break;
                    }

                    if ctx.break_requested().is_some() {
                        ctx.decrement_break();
                        break;
                    }

                    if ctx.continue_requested().is_some() {
                        let cont_depth = ctx.continue_requested().unwrap();
                        ctx.decrement_continue();
                        if cont_depth > 1 {
                            break;
                        }
                        continue;
                    }
                }

                ctx.exit_loop();
                Ok(last_status)
            }

            ExecutionStage::CaseBranch { target, arms } => {
                let target_val = expand_word(target, ctx).await?;
                if ctx.is_cancelled() {
                    ctx.set_last_exit(130);
                    return Ok(130);
                }
                let mut last_status = 0;
                let mut fallthrough = false;

                for arm in arms {
                    let mut matched = fallthrough;

                    if !matched {
                        for pat in &arm.patterns {
                            let pat_str = expand_case_pattern(pat, ctx).await?;
                            if ctx.is_cancelled() {
                                ctx.set_last_exit(130);
                                return Ok(130);
                            }
                            if shell_pattern_matches(&pat_str, &target_val) {
                                matched = true;
                                break;
                            }
                        }
                    }

                    if matched {
                        if let Some(body) = &arm.body {
                            last_status = body.execute(ctx).await?;
                            ctx.set_last_exit(last_status);
                        }

                        if ctx.is_exiting()
                            || ctx.is_returning()
                            || ctx.break_requested().is_some()
                            || ctx.continue_requested().is_some()
                            || ctx.is_cancelled()
                        {
                            break;
                        }

                        match arm.delimiter {
                            crate::parser::ast::CaseDelimiter::Break => {
                                break;
                            }
                            crate::parser::ast::CaseDelimiter::Fallthrough => {
                                fallthrough = true;
                            }
                            crate::parser::ast::CaseDelimiter::ResumeNext => {
                                fallthrough = false;
                            }
                        }
                    } else {
                        fallthrough = false;
                    }
                }

                Ok(last_status)
            }
        }
    }
}

async fn execute_redirected_compound(
    inner: &ExecutionStage,
    redirections: &[PlanRedirection],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    let scopes_stdin = redirections.iter().any(|redirection| {
        matches!(
            redirection.op,
            RedirectionOp::RedirectIn
                | RedirectionOp::RedirectHereString
                | RedirectionOp::RedirectHereDoc
                | RedirectionOp::RedirectHereDocStrip
        )
    });
    let previous_stdin = scopes_stdin.then(|| ctx.take_stdin()).flatten();
    let result = execute_redirected_compound_inner(inner, redirections, ctx).await;
    if scopes_stdin {
        let _ = ctx.take_stdin();
        if let Some(stdin) = previous_stdin {
            ctx.set_stdin(stdin);
        }
    }
    result
}

async fn execute_redirected_compound_inner(
    inner: &ExecutionStage,
    redirections: &[PlanRedirection],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    let mut output_targets = vec![None; redirections.len()];
    let mut has_output_redirections = false;

    for (index, redirection) in redirections.iter().enumerate() {
        match redirection.op {
            RedirectionOp::RedirectIn => {
                let target_path = expand_word(&redirection.target, ctx).await?;
                let target =
                    StreamTarget::from_redirection(redirection.op, &target_path, ctx.cwd())?;
                match target {
                    StreamTarget::Null => ctx.set_stdin(Vec::new()),
                    StreamTarget::File { path, .. } => {
                        let content = fs::read(&path).map_err(|error| {
                            ExecError::NotFound(format!(
                                "Cannot open input file '{}': {}",
                                path.display(),
                                error
                            ))
                        })?;
                        ctx.set_stdin(content);
                    }
                    StreamTarget::Inherit | StreamTarget::Stderr | StreamTarget::Stdout => {}
                }
            }
            RedirectionOp::RedirectHereString => {
                let content = expand_word(&redirection.target, ctx).await?;
                ctx.set_stdin(format!("{}\n", content).into_bytes());
            }
            RedirectionOp::RedirectHereDoc | RedirectionOp::RedirectHereDocStrip => {
                let content = expand_word(&redirection.target, ctx).await?;
                ctx.set_stdin(content.into_bytes());
            }
            RedirectionOp::RedirectOut
            | RedirectionOp::RedirectClobber
            | RedirectionOp::RedirectAppend
            | RedirectionOp::RedirectStderr
            | RedirectionOp::RedirectStderrAppend
            | RedirectionOp::RedirectAll
            | RedirectionOp::RedirectAllAppend => {
                has_output_redirections = true;
                let target_path = expand_word(&redirection.target, ctx).await?;
                let target =
                    StreamTarget::from_redirection(redirection.op, &target_path, ctx.cwd())?;
                let noclobber =
                    ctx.options().noclobber && redirection.op == RedirectionOp::RedirectOut;
                target.prepare_for_write(noclobber)?;
                output_targets[index] = Some(target);
            }
            RedirectionOp::RedirectStderrToStdout | RedirectionOp::RedirectStdoutToStderr => {
                has_output_redirections = true;
            }
        }
    }

    let (stdout_destination, stderr_destination) =
        output_destinations(redirections, &output_targets);
    let (execution_result, captured_out, captured_err) = if has_output_redirections {
        let previous_capture = ctx.begin_redirect_capture(stdout_destination == stderr_destination);
        let result = inner.execute(ctx).await;
        let (stdout, stderr) = ctx.finish_redirect_capture(previous_capture);
        (result, Some(stdout), Some(stderr))
    } else {
        (inner.execute(ctx).await, None, None)
    };

    if has_output_redirections {
        let stdout_bytes = captured_out.unwrap_or_default();
        let stderr_bytes = captured_err.unwrap_or_default();
        if stdout_destination == stderr_destination {
            write_to_destination(&stdout_destination, &stdout_bytes, ctx)?;
        } else {
            write_to_destination(&stdout_destination, &stdout_bytes, ctx)?;
            write_to_destination(&stderr_destination, &stderr_bytes, ctx)?;
        }
    }

    let status = execution_result?;
    ctx.set_last_exit(status);
    Ok(status)
}

async fn execute_pipeline(
    nodes: &[PlanNode],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    if nodes.is_empty() {
        return Ok(0);
    }

    if nodes.len() == 1 {
        return execute_plan_node(&nodes[0], ctx).await;
    }

    if let Some(status) = execute_external_pipeline_to_builtin_consumer(nodes, ctx).await? {
        return Ok(status);
    }

    if let Some(status) = execute_streaming_external_pipeline(nodes, ctx).await? {
        return Ok(status);
    }

    // Pipeline: node0 | node1 | ... | nodeN
    let mut current_input: Option<Vec<u8>> = ctx.take_stdin();
    let mut statuses = Vec::new();

    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == nodes.len() - 1;
        let mut step_ctx = ctx.clone_isolated();

        if let Some(in_bytes) = current_input.take() {
            step_ctx.set_stdin(in_bytes);
        }

        step_ctx.enable_capture();

        let status = execute_isolated_plan_node(node, &mut step_ctx).await?;
        statuses.push(status);

        if !is_last {
            current_input = step_ctx.get_captured_stdout_bytes();
            if let Some(stderr) = step_ctx.get_captured_stderr_bytes() {
                ctx.write_stderr(&stderr);
            }
        } else {
            if let Some(out) = step_ctx.get_captured_stdout_bytes() {
                ctx.write_stdout(&out);
            }
            if let Some(err) = step_ctx.get_captured_stderr_bytes() {
                ctx.write_stderr(&err);
            }
            // Propagate lastpipe state (scopes, env vars, interactive events)
            ctx.adopt_lastpipe_state(step_ctx);

            let final_status = if ctx.options().pipefail {
                statuses.into_iter().rev().find(|&s| s != 0).unwrap_or(0)
            } else {
                status
            };
            ctx.set_last_exit(final_status);
            return Ok(final_status);
        }
    }

    Ok(0)
}

/// Stream external stages into ESH builtins that can stop after a prefix.
/// These builtins take buffered stdin, so collect only the prefix they need and
/// drop the upstream receiver as soon as they can finish.
async fn execute_external_pipeline_to_builtin_consumer(
    nodes: &[PlanNode],
    ctx: &mut ExecutionContext,
) -> Result<Option<i32>, ExecError> {
    let Some(consumer_node) = nodes.last() else {
        return Ok(None);
    };
    let head_index = nodes.len() - 1;
    let consumer = if let Some(limit) = builtin_head_limit(consumer_node) {
        BuiltinStreamConsumer::Head(limit)
    } else if builtin_quiet_grep(consumer_node) {
        BuiltinStreamConsumer::QuietGrep
    } else {
        return Ok(None);
    };
    let upstream = &nodes[..head_index];
    if upstream.is_empty()
        || ctx.get_function(&consumer_node.raw_name).is_some()
        || ctx.external_file_outputs().is_some()
        || ctx.is_in_trap()
        || ctx.options().xtrace
        || [TrapSignal::Exit, TrapSignal::Err, TrapSignal::Interrupt]
            .iter()
            .any(|signal| ctx.get_trap(*signal).is_some())
    {
        return Ok(None);
    }

    let mut external_nodes = Vec::with_capacity(upstream.len());
    let mut external_node_indexes = Vec::with_capacity(upstream.len());
    for (index, node) in upstream.iter().enumerate() {
        if builtin_passthrough_cat(node, ctx) {
            continue;
        }
        if !matches!(node.target, CommandTarget::External(_))
            || !node.assignments.is_empty()
            || !node.redirections.is_empty()
            || ctx.get_function(&node.raw_name).is_some()
            || node.args.iter().any(|word| {
                word.parts.iter().any(|part| {
                    !matches!(
                        part,
                        PlanWordPart::Literal(_) | PlanWordPart::SingleQuoted(_)
                    )
                })
            })
        {
            return Ok(None);
        }
        external_nodes.push(node);
        external_node_indexes.push(index);
    }
    if external_nodes.is_empty() {
        return Ok(None);
    }

    let mut programs = Vec::with_capacity(external_nodes.len());
    for node in &external_nodes {
        let Some(program) = resolve_pipeline_program(&node.raw_name, ctx) else {
            return Ok(None);
        };
        programs.push(program);
    }
    let arguments = external_nodes
        .iter()
        .map(|node| {
            node.args
                .iter()
                .map(|word| {
                    word.parts
                        .iter()
                        .map(|part| match part {
                            PlanWordPart::Literal(value) | PlanWordPart::SingleQuoted(value) => {
                                value.as_str()
                            }
                            _ => unreachable!("eligibility checked above"),
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let mut handles = Vec::with_capacity(external_nodes.len());
    let mut previous_output = None;
    let mut initial_input = ctx.take_stdin();
    let mut final_receiver = None;
    for (index, (program, args)) in programs.into_iter().zip(arguments).enumerate() {
        let is_last_upstream = index + 1 == external_nodes.len();
        let mut spec = ProcessSpec::new(program, ctx.cwd(), ProcessMode::SupervisedAutomation)
            .args(args)
            .environment(EnvironmentPolicy::InheritAndOverride, ctx.env_vars());
        spec.stdin = if index == 0 {
            initial_input
                .take()
                .map(StdinPolicy::Bytes)
                .unwrap_or(StdinPolicy::Null)
        } else {
            StdinPolicy::Pipe
        };
        spec.stdout = OutputPolicy::Stream;
        spec.stderr = OutputPolicy::Capture;
        let (sender, receiver) = tokio::sync::mpsc::channel::<OutputChunk>(8);
        let mut handle = ProcessHandle::spawn_streaming(spec, sender)
            .map_err(|error| ExecError::IoError(error.to_string()))?;
        if let Some(receiver) = previous_output.take() {
            handle.connect_stdin_from(receiver);
        }
        if is_last_upstream {
            final_receiver = Some(receiver);
        } else {
            previous_output = Some(receiver);
        }
        handles.push(handle);
    }

    let mut input = Vec::new();
    let mut grep_probe_offset = 0;
    let mut grep_match_found = false;
    let mut receiver = final_receiver.expect("upstream pipeline has a final receiver");
    while let Some(chunk) = receiver.recv().await {
        if chunk.stream != evin_process::OutputStream::Stdout {
            continue;
        }
        match consumer {
            BuiltinStreamConsumer::Head(limit) => {
                let remaining = match limit {
                    HeadLimit::Bytes(count) => count.saturating_sub(input.len()),
                    HeadLimit::Lines(0) => 0,
                    HeadLimit::Lines(count) => {
                        let seen = input.iter().filter(|byte| **byte == b'\n').count();
                        if seen >= count { 0 } else { chunk.bytes.len() }
                    }
                };
                if remaining == 0 {
                    break;
                }
                match limit {
                    HeadLimit::Bytes(_) => {
                        input.extend_from_slice(&chunk.bytes[..chunk.bytes.len().min(remaining)])
                    }
                    HeadLimit::Lines(count) => {
                        let mut take = chunk.bytes.len();
                        let mut seen = input.iter().filter(|byte| **byte == b'\n').count();
                        for (index, byte) in chunk.bytes.iter().enumerate() {
                            if *byte == b'\n' {
                                seen += 1;
                                if seen == count {
                                    take = index + 1;
                                    break;
                                }
                            }
                        }
                        input.extend_from_slice(&chunk.bytes[..take]);
                    }
                }
                if match limit {
                    HeadLimit::Bytes(count) => input.len() >= count,
                    HeadLimit::Lines(count) => {
                        input.iter().filter(|byte| **byte == b'\n').count() >= count
                    }
                } {
                    break;
                }
            }
            BuiltinStreamConsumer::QuietGrep => {
                input.extend_from_slice(&chunk.bytes);
                while let Some(relative_end) = input[grep_probe_offset..]
                    .iter()
                    .position(|byte| *byte == b'\n')
                {
                    let line_end = grep_probe_offset + relative_end + 1;
                    let mut probe_ctx = ctx.clone_isolated();
                    probe_ctx.set_stdin(input[grep_probe_offset..line_end].to_vec());
                    probe_ctx.enable_capture();
                    grep_match_found =
                        execute_isolated_plan_node(consumer_node, &mut probe_ctx).await? == 0;
                    grep_probe_offset = line_end;
                    if grep_match_found {
                        break;
                    }
                }
                if grep_match_found {
                    break;
                }
            }
        }
    }
    drop(receiver);

    let mut consumer_ctx = ctx.clone_isolated();
    consumer_ctx.set_stdin(input);
    consumer_ctx.enable_capture();
    let consumer_status = execute_isolated_plan_node(consumer_node, &mut consumer_ctx).await?;
    if let Some(stdout) = consumer_ctx.get_captured_stdout_bytes() {
        ctx.write_stdout(&stdout);
    }
    if let Some(stderr) = consumer_ctx.get_captured_stderr_bytes() {
        ctx.write_stderr(&stderr);
    }

    let mut join_set = JoinSet::new();
    for (index, handle) in handles.into_iter().enumerate() {
        let token = ctx.token().clone();
        join_set.spawn(async move { (index, handle.wait(token).await) });
    }
    let mut statuses = vec![0; upstream.len()];
    while let Some(result) = join_set.join_next().await {
        let (index, result) = result.map_err(|error| ExecError::JoinError(error.to_string()))?;
        let result = result.map_err(|error| ExecError::IoError(error.to_string()))?;
        statuses[external_node_indexes[index]] = match result.termination {
            ProcessTermination::Exited(status) => external_process_exit_code(status),
            ProcessTermination::Cancelled => 130,
        };
        if let Some(stderr) = result.stderr {
            ctx.write_stderr(&stderr);
        }
    }
    statuses.push(consumer_status);
    let status = if ctx.options().pipefail {
        statuses
            .into_iter()
            .rev()
            .find(|status| *status != 0)
            .unwrap_or(0)
    } else {
        consumer_status
    };
    ctx.set_last_exit(status);
    Ok(Some(status))
}

#[derive(Clone, Copy)]
enum BuiltinStreamConsumer {
    Head(HeadLimit),
    QuietGrep,
}

fn builtin_passthrough_cat(node: &PlanNode, ctx: &ExecutionContext) -> bool {
    node.raw_name == "cat"
        && matches!(node.target, CommandTarget::Builtin(_))
        && node.args.is_empty()
        && node.assignments.is_empty()
        && node.redirections.is_empty()
        && ctx.get_function(&node.raw_name).is_none()
}

#[derive(Clone, Copy)]
enum HeadLimit {
    Lines(usize),
    Bytes(usize),
}

fn builtin_head_limit(node: &PlanNode) -> Option<HeadLimit> {
    if node.raw_name != "head"
        || !matches!(node.target, CommandTarget::Builtin(_))
        || !node.assignments.is_empty()
        || !node.redirections.is_empty()
    {
        return None;
    }
    let mut mode = HeadLimit::Lines(10);
    let mut parsing_options = true;
    let mut iter = node.args.iter();
    while let Some(word) = iter.next() {
        let value = match word.parts.as_slice() {
            [PlanWordPart::Literal(value) | PlanWordPart::SingleQuoted(value)] => value,
            _ => return None,
        };
        if parsing_options && value == "--" {
            parsing_options = false;
        } else if parsing_options && (value == "-n" || value == "--lines") {
            let [PlanWordPart::Literal(count) | PlanWordPart::SingleQuoted(count)] =
                iter.next()?.parts.as_slice()
            else {
                return None;
            };
            mode = HeadLimit::Lines(count.parse().ok()?);
        } else if parsing_options && (value == "-c" || value == "--bytes") {
            let [PlanWordPart::Literal(count) | PlanWordPart::SingleQuoted(count)] =
                iter.next()?.parts.as_slice()
            else {
                return None;
            };
            mode = HeadLimit::Bytes(count.parse().ok()?);
        } else if parsing_options && value.starts_with('-') && value != "-" {
            return None;
        } else {
            // File operands do not consume stdin, so this optimization would
            // need to drain the pipeline instead of stopping at the limit.
            return None;
        }
    }
    Some(mode)
}

fn builtin_quiet_grep(node: &PlanNode) -> bool {
    if node.raw_name != "grep"
        || !matches!(node.target, CommandTarget::Builtin(_))
        || !node.assignments.is_empty()
        || !node.redirections.is_empty()
        || node.args.len() != 2
    {
        return false;
    }
    let is_static_word = |word: &PlanWord| {
        matches!(
            word.parts.as_slice(),
            [PlanWordPart::Literal(_) | PlanWordPart::SingleQuoted(_)]
        )
    };
    if !node.args.iter().all(is_static_word) {
        return false;
    }
    matches!(
        node.args[0].parts.as_slice(),
        [PlanWordPart::Literal(option) | PlanWordPart::SingleQuoted(option)]
            if matches!(option.as_str(), "-q" | "-Fq" | "-qF")
    )
}

async fn execute_streaming_external_pipeline(
    nodes: &[PlanNode],
    ctx: &mut ExecutionContext,
) -> Result<Option<i32>, ExecError> {
    if ctx.external_file_outputs().is_some()
        || ctx.is_in_trap()
        || ctx.options().xtrace
        || [TrapSignal::Exit, TrapSignal::Err, TrapSignal::Interrupt]
            .iter()
            .any(|signal| ctx.get_trap(*signal).is_some())
        || nodes.iter().any(|node| {
            !matches!(node.target, CommandTarget::External(_))
                || !node.assignments.is_empty()
                || !node.redirections.is_empty()
                || ctx.get_function(&node.raw_name).is_some()
                || node.args.iter().any(|word| {
                    word.parts.iter().any(|part| {
                        !matches!(
                            part,
                            PlanWordPart::Literal(_) | PlanWordPart::SingleQuoted(_)
                        )
                    })
                })
        })
    {
        return Ok(None);
    }

    let mut programs = Vec::with_capacity(nodes.len());
    for node in nodes {
        let Some(program) = resolve_pipeline_program(&node.raw_name, ctx) else {
            return Ok(None);
        };
        programs.push(program);
    }

    let mut arguments = Vec::with_capacity(nodes.len());
    for node in nodes {
        arguments.push(
            node.args
                .iter()
                .map(|word| {
                    word.parts
                        .iter()
                        .map(|part| match part {
                            PlanWordPart::Literal(value) | PlanWordPart::SingleQuoted(value) => {
                                value.as_str()
                            }
                            _ => unreachable!("eligibility checked above"),
                        })
                        .collect::<String>()
                })
                .collect::<Vec<_>>(),
        );
    }

    let mut handles = Vec::with_capacity(nodes.len());
    let mut previous_output = None;
    let mut input = ctx.take_stdin();
    for (index, (program, args)) in programs.into_iter().zip(arguments).enumerate() {
        let is_last = index + 1 == nodes.len();
        let mut spec = ProcessSpec::new(program, ctx.cwd(), ProcessMode::SupervisedAutomation)
            .args(args)
            .environment(EnvironmentPolicy::InheritAndOverride, ctx.env_vars());
        spec.stdin = if index == 0 {
            input
                .take()
                .map(StdinPolicy::Bytes)
                .unwrap_or(StdinPolicy::Null)
        } else {
            StdinPolicy::Pipe
        };
        spec.stdout = if is_last {
            OutputPolicy::Capture
        } else {
            OutputPolicy::Stream
        };
        spec.stderr = OutputPolicy::Capture;

        let (sender, receiver) = if is_last {
            (None, None)
        } else {
            let (sender, receiver) = tokio::sync::mpsc::channel::<OutputChunk>(8);
            (Some(sender), Some(receiver))
        };
        let mut handle = if let Some(sender) = sender {
            ProcessHandle::spawn_streaming(spec, sender)
        } else {
            ProcessHandle::spawn(spec, None)
        }
        .map_err(|error| ExecError::IoError(error.to_string()))?;
        if let Some(receiver) = previous_output.take() {
            handle.connect_stdin_from(receiver);
        }
        previous_output = receiver;
        handles.push(handle);
    }

    let mut join_set = JoinSet::new();
    for (index, handle) in handles.into_iter().enumerate() {
        let token = ctx.token().clone();
        join_set.spawn(async move { (index, handle.wait(token).await) });
    }

    let mut results = std::iter::repeat_with(|| None)
        .take(nodes.len())
        .collect::<Vec<Option<Result<evin_process::ProcessResult, ProcessError>>>>();
    while let Some(result) = join_set.join_next().await {
        let (index, result) = result.map_err(|error| ExecError::JoinError(error.to_string()))?;
        results[index] = Some(result);
    }

    let mut statuses = Vec::with_capacity(nodes.len());
    let mut final_stdout = Vec::new();
    for (index, result) in results.into_iter().enumerate() {
        let result = result
            .expect("all pipeline process tasks joined")
            .map_err(|error| ExecError::IoError(error.to_string()))?;
        let status = match result.termination {
            ProcessTermination::Exited(status) => external_process_exit_code(status),
            ProcessTermination::Cancelled => 130,
        };
        statuses.push(status);
        if let Some(stderr) = result.stderr {
            ctx.write_stderr(&stderr);
        }
        if index + 1 == nodes.len()
            && let Some(stdout) = result.stdout
        {
            final_stdout = stdout;
        }
    }

    ctx.write_stdout(&final_stdout);
    let status = if ctx.options().pipefail {
        statuses
            .into_iter()
            .rev()
            .find(|status| *status != 0)
            .unwrap_or(0)
    } else {
        statuses.last().copied().unwrap_or(0)
    };
    ctx.set_last_exit(status);
    Ok(Some(status))
}

fn resolve_pipeline_program(raw_name: &str, ctx: &ExecutionContext) -> Option<PathBuf> {
    let requested = Path::new(raw_name);
    if requested.is_absolute() {
        return crate::runtime::builtins::command::is_executable_candidate(requested)
            .then(|| requested.to_path_buf());
    }
    if raw_name.contains('/') || (cfg!(windows) && raw_name.contains('\\')) {
        let path = ctx.cwd().join(requested);
        return crate::runtime::builtins::command::is_executable_candidate(&path).then_some(path);
    }
    let Some(path_var) = ctx.get_var("PATH") else {
        let path = ctx.cwd().join(requested);
        return crate::runtime::builtins::command::is_executable_candidate(&path).then_some(path);
    };
    std::env::split_paths(&path_var)
        .map(|directory| {
            if directory.is_absolute() {
                directory.join(raw_name)
            } else {
                ctx.cwd().join(directory).join(raw_name)
            }
        })
        .find(|path| crate::runtime::builtins::command::is_executable_candidate(path))
}

async fn execute_isolated_plan_node(
    node: &PlanNode,
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    ctx.enter_exec();
    let result = execute_plan_node(node, ctx).await;
    ctx.leave_exec();
    execute_exit_trap(ctx).await;
    result
}

async fn execute_isolated_stage(
    stage: &ExecutionStage,
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    ctx.enter_exec();
    let result = stage.execute(ctx).await;
    ctx.leave_exec();
    execute_exit_trap(ctx).await;
    result
}

async fn execute_compound_pipeline(
    stages: &[ExecutionStage],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    if stages.is_empty() {
        return Ok(0);
    }
    if stages.len() == 1 {
        return stages[0].execute(ctx).await;
    }

    if let Some(nodes) = simple_builtin_consumer_pipeline(stages)
        && let Some(status) = execute_external_pipeline_to_builtin_consumer(&nodes, ctx).await?
    {
        return Ok(status);
    }

    let mut current_input = ctx.take_stdin();
    let mut statuses = Vec::with_capacity(stages.len());
    for (index, stage) in stages.iter().enumerate() {
        let is_last = index + 1 == stages.len();
        let mut step_ctx = ctx.clone_isolated();
        if let Some(input) = current_input.take() {
            step_ctx.set_stdin(input);
        }
        step_ctx.enable_capture();
        let status = execute_isolated_stage(stage, &mut step_ctx).await?;
        statuses.push(status);

        if is_last {
            if let Some(stdout) = step_ctx.get_captured_stdout_bytes() {
                ctx.write_stdout(&stdout);
            }
            if let Some(stderr) = step_ctx.get_captured_stderr_bytes() {
                ctx.write_stderr(&stderr);
            }
            ctx.adopt_lastpipe_state(step_ctx);
            let final_status = if ctx.options().pipefail {
                statuses
                    .into_iter()
                    .rev()
                    .find(|&code| code != 0)
                    .unwrap_or(0)
            } else {
                status
            };
            ctx.set_last_exit(final_status);
            return Ok(final_status);
        }

        current_input = step_ctx.get_captured_stdout_bytes();
        if let Some(stderr) = step_ctx.get_captured_stderr_bytes() {
            ctx.write_stderr(&stderr);
        }
    }
    Ok(0)
}

fn simple_builtin_consumer_pipeline(stages: &[ExecutionStage]) -> Option<Vec<PlanNode>> {
    let (consumer_stage, upstream_stages) = stages.split_last()?;
    let consumer_node = single_simple_pipeline_node(consumer_stage)?;
    if !matches!(consumer_node.target, CommandTarget::Builtin(_)) {
        return None;
    }

    let mut nodes = Vec::new();
    for stage in upstream_stages {
        let ExecutionStage::Pipeline(stage_nodes) = stage else {
            return None;
        };
        nodes.extend(stage_nodes.iter().cloned());
    }
    if nodes.is_empty() {
        return None;
    }
    nodes.push(consumer_node.clone());
    Some(nodes)
}

fn single_simple_pipeline_node(stage: &ExecutionStage) -> Option<&PlanNode> {
    match stage {
        ExecutionStage::Pipeline(nodes) if nodes.len() == 1 => nodes.first(),
        ExecutionStage::Block(stages) if stages.len() == 1 => {
            single_simple_pipeline_node(&stages[0])
        }
        _ => None,
    }
}

fn expand_for_array(word: &PlanWord, ctx: &ExecutionContext) -> Option<Vec<String>> {
    if matches!(
        word.parts.as_slice(),
        [PlanWordPart::Variable(name)] if name == "@"
    ) || matches!(
        word.parts.as_slice(),
        [PlanWordPart::DoubleQuoted(content)] if content == "$@" || content == "${@}"
    ) {
        return Some(ctx.positional_args().to_vec());
    }
    let expression = match word.parts.as_slice() {
        [PlanWordPart::Variable(expression)] => expression.as_str(),
        [PlanWordPart::DoubleQuoted(content)] => content
            .strip_prefix("${")
            .and_then(|content| content.strip_suffix('}'))?,
        _ => return None,
    };
    let name = expression.strip_suffix("[@]")?;
    if let Some(name) = name.strip_prefix('!') {
        if !is_valid_var_ident(name) {
            return None;
        }
        return Some(
            ctx.get_shell_val(name)
                .map(|value| value.get_keys())
                .unwrap_or_default(),
        );
    }
    if !is_valid_var_ident(name) {
        return None;
    }
    Some(
        ctx.get_shell_val(name)
            .map(|value| value.get_values())
            .unwrap_or_default(),
    )
}

#[derive(Clone, Copy)]
enum ParameterPatternRemoval {
    Prefix { longest: bool },
    Suffix { longest: bool },
}

fn parse_parameter_pattern_removal(expression: &str) -> Option<(ParameterPatternRemoval, &str)> {
    for (operator, removal) in [
        ("##", ParameterPatternRemoval::Prefix { longest: true }),
        ("#", ParameterPatternRemoval::Prefix { longest: false }),
        ("%%", ParameterPatternRemoval::Suffix { longest: true }),
        ("%", ParameterPatternRemoval::Suffix { longest: false }),
    ] {
        if let Some(pattern) = expression.strip_prefix(operator) {
            return Some((removal, pattern));
        }
    }
    None
}

fn apply_parameter_pattern_removal(
    value: &str,
    pattern: &str,
    removal: ParameterPatternRemoval,
) -> String {
    match removal {
        ParameterPatternRemoval::Prefix { longest } => {
            strip_prefix_pattern(value, pattern, longest)
        }
        ParameterPatternRemoval::Suffix { longest } => {
            strip_suffix_pattern(value, pattern, longest)
        }
    }
}

async fn expand_word_as_items(
    word: &PlanWord,
    ctx: &mut ExecutionContext,
) -> Result<Option<Vec<String>>, ExecError> {
    if let Some(items) = expand_for_array(word, ctx) {
        return Ok(Some(items));
    }

    let (expression, double_quoted) = match word.parts.as_slice() {
        [PlanWordPart::Variable(expression)] => (expression.as_str(), false),
        [PlanWordPart::DoubleQuoted(content)] => {
            let Some(expression) = content
                .strip_prefix("${")
                .and_then(|content| content.strip_suffix('}'))
            else {
                return Ok(None);
            };
            (expression, true)
        }
        _ => return Ok(None),
    };

    if let Some((name, subscript_and_modifier)) = expression.split_once('[')
        && is_valid_var_ident(name)
        && let Some((subscript, modifier)) = subscript_and_modifier.split_once(']')
        && subscript == "@"
        && let Some((removal, pattern_expression)) = parse_parameter_pattern_removal(modifier)
    {
        let Some(values) = ctx.get_shell_val(name).map(|value| value.get_values()) else {
            if ctx.options().nounset {
                return Err(ExecError::Custom(format!("{}: unbound variable", name)));
            }
            return Ok(Some(Vec::new()));
        };
        let pattern = expand_parameter_pattern(pattern_expression, ctx).await?;
        return Ok(Some(
            values
                .iter()
                .map(|element| apply_parameter_pattern_removal(element, &pattern, removal))
                .collect(),
        ));
    }

    if let Some((name, subscript_and_modifier)) = expression.split_once('[')
        && is_valid_var_ident(name)
        && let Some((subscript, modifier)) = subscript_and_modifier.split_once(']')
        && subscript == "@"
        && let Some(slice_spec) = modifier.strip_prefix(':')
    {
        let Some(value) = ctx.get_shell_val(name) else {
            if ctx.options().nounset {
                return Err(ExecError::Custom(format!("{}: unbound variable", name)));
            }
            return Ok(Some(Vec::new()));
        };
        return Ok(Some(slice_parameter_array_items(value, slice_spec)?));
    }

    let (operator, operand) = if let Some(operand) = expression.strip_prefix("@:-") {
        (":-", operand)
    } else if let Some(operand) = expression.strip_prefix("@-") {
        ("-", operand)
    } else if let Some(operand) = expression.strip_prefix("@:+") {
        (":+", operand)
    } else if let Some(operand) = expression.strip_prefix("@+") {
        ("+", operand)
    } else {
        if let Some(pattern_expression) = expression.strip_prefix('@')
            && let Some((removal, pattern_expression)) =
                parse_parameter_pattern_removal(pattern_expression)
        {
            let pattern = expand_parameter_pattern(pattern_expression, ctx).await?;
            return Ok(Some(
                ctx.positional_args()
                    .iter()
                    .map(|value| apply_parameter_pattern_removal(value, &pattern, removal))
                    .collect(),
            ));
        }
        return Ok(None);
    };

    let positional = ctx.positional_args();
    let joined = positional.join(" ");
    let take_parameter = match operator {
        ":-" => !joined.is_empty(),
        "-" => !positional.is_empty(),
        ":+" => !joined.is_empty(),
        "+" => !positional.is_empty(),
        _ => unreachable!(),
    };
    let is_default = operator == ":-" || operator == "-";

    if is_default {
        if take_parameter {
            return Ok(Some(positional.to_vec()));
        }
        return Ok(Some(vec![
            expand_parameter_operand(operand, ctx, double_quoted).await?,
        ]));
    }
    if take_parameter {
        return Ok(Some(vec![
            expand_parameter_operand(operand, ctx, double_quoted).await?,
        ]));
    }
    Ok(Some(positional.to_vec()))
}

async fn execute_plan_node(node: &PlanNode, ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
    let scope_assignments = !node.assignments.is_empty() && node.raw_name != "=";
    let scope_stdin = node.redirections.iter().any(|redirection| {
        matches!(
            redirection.op,
            RedirectionOp::RedirectIn
                | RedirectionOp::RedirectHereString
                | RedirectionOp::RedirectHereDoc
                | RedirectionOp::RedirectHereDocStrip
        )
    });

    if !scope_assignments && !scope_stdin {
        return execute_plan_node_inner(node, ctx).await;
    }

    let previous_env = scope_assignments.then(|| {
        node.assignments
            .iter()
            .map(|assignment| {
                (
                    assignment.name.clone(),
                    (
                        ctx.env_vars().get(&assignment.name).cloned(),
                        ctx.is_exported(&assignment.name),
                    ),
                )
            })
            .collect()
    });
    let previous_cwd = scope_assignments.then(|| ctx.cwd().to_path_buf());
    if scope_assignments {
        ctx.push_command_assignment_scope();
    }
    let previous_stdin = if scope_stdin { ctx.take_stdin() } else { None };
    let result = execute_plan_node_inner(node, ctx).await;

    if scope_stdin {
        let _ = ctx.take_stdin();
        if let Some(stdin) = previous_stdin {
            ctx.set_stdin(stdin);
        }
    }
    if let Some(previous_env) = previous_env {
        ctx.restore_env_vars(previous_env);
        let _ = ctx.pop_scope();
    }
    if let Some(previous_cwd) = previous_cwd
        && previous_cwd != ctx.cwd()
    {
        ctx.set_exported_var(
            "OLDPWD",
            crate::runtime::path_presentation::present(&previous_cwd),
        );
        ctx.set_exported_var("PWD", crate::runtime::path_presentation::present(ctx.cwd()));
    }
    result
}

async fn execute_plan_node_inner(
    node: &PlanNode,
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    let (command_name, target) = match &node.target {
        CommandTarget::Dynamic(command_word) => {
            // The command word expands before prefix assignments become visible.
            let command_name = expand_word(command_word, ctx).await?;
            let target = resolve_builtin(&command_name)
                .map(CommandTarget::Builtin)
                .unwrap_or_else(|| {
                    CommandTarget::External(std::path::PathBuf::from(&command_name))
                });
            (command_name, target)
        }
        _ => (node.raw_name.clone(), node.target.clone()),
    };
    let substitution_count_before_assignments =
        (node.raw_name == "=").then(|| ctx.command_substitution_count());

    // Apply command-scoped assignments before redirection and argument expansion.
    // Keep the prior status available while assignment values are expanded;
    // the command wrapper records this assignment-only command's status after
    // execution completes.
    for asgn in &node.assignments {
        if let Some(array_vals) = &asgn.array_values {
            let mut items = Vec::new();
            for v in array_vals {
                items.push(expand_word(v, ctx).await?);
            }
            let value =
                crate::runtime::context::ShellValue::Array(items.into_iter().enumerate().collect());
            if node.raw_name == "=" {
                ctx.set_scoped_val(&asgn.name, value);
            } else {
                ctx.set_local_val(&asgn.name, value);
            }
        } else if let Some(idx_word) = &asgn.index {
            let sub = expand_word(idx_word, ctx).await?;
            let val = if let Some(v) = &asgn.value {
                expand_word(v, ctx).await?
            } else {
                String::new()
            };
            let is_map = matches!(
                ctx.get_shell_val(&asgn.name),
                Some(crate::runtime::context::ShellValue::Map(_))
            );
            if !is_map && let Ok(num_idx) = sub.parse::<usize>() {
                if node.raw_name == "=" {
                    ctx.set_array_elem(&asgn.name, num_idx, val);
                } else {
                    ctx.set_local_val(
                        &asgn.name,
                        crate::runtime::context::ShellValue::Array(
                            std::collections::BTreeMap::from([(num_idx, val)]),
                        ),
                    );
                }
            } else {
                if node.raw_name == "=" {
                    ctx.set_map_elem(&asgn.name, sub, val);
                } else {
                    let mut map = rustc_hash::FxHashMap::default();
                    map.insert(sub, val);
                    ctx.set_local_val(&asgn.name, crate::runtime::context::ShellValue::Map(map));
                }
            }
        } else if let Some(val_word) = &asgn.value {
            let val = expand_word(val_word, ctx).await?;
            if node.raw_name == "=" {
                ctx.set_scoped_var(&asgn.name, val);
            } else {
                ctx.set_local_var(&asgn.name, val);
            }
        }
    }

    let assignment_only_status = substitution_count_before_assignments.map(|before| {
        if ctx.command_substitution_count() != before {
            ctx.last_exit()
        } else {
            0
        }
    });

    // Prefix assignments form the environment of this command. The outer
    // execution wrapper scopes the shell values and restores exported values.
    if node.raw_name != "=" {
        for asgn in &node.assignments {
            if asgn.index.is_none()
                && asgn.array_values.is_none()
                && let Some(value) = ctx.get_var(&asgn.name)
            {
                ctx.set_env(asgn.name.clone(), value);
            }
        }
    }

    // Redirections are expanded and opened from left to right before dispatch.
    let mut expanded_output_targets = vec![None; node.redirections.len()];
    for (index, redir) in node.redirections.iter().enumerate() {
        if redir.op == RedirectionOp::RedirectIn {
            let target_path = expand_word(&redir.target, ctx).await?;
            let target = match StreamTarget::from_redirection(redir.op, &target_path, ctx.cwd()) {
                Ok(target) => target,
                Err(error) => {
                    let (_, stderr_destination) = output_destinations(
                        &node.redirections[..index],
                        &expanded_output_targets[..index],
                    );
                    return redirection_failure(
                        error,
                        &command_name,
                        node.span.line,
                        &stderr_destination,
                        ctx,
                    )
                    .await;
                }
            };
            match target {
                StreamTarget::Null => ctx.set_stdin(Vec::new()),
                StreamTarget::File { path, .. } => {
                    let content = match fs::read(&path) {
                        Ok(content) => content,
                        Err(error) => {
                            let error = ExecError::NotFound(format!(
                                "Cannot open input file '{}': {}",
                                path.display(),
                                error
                            ));
                            let (_, stderr_destination) = output_destinations(
                                &node.redirections[..index],
                                &expanded_output_targets[..index],
                            );
                            return redirection_failure(
                                error,
                                &command_name,
                                node.span.line,
                                &stderr_destination,
                                ctx,
                            )
                            .await;
                        }
                    };
                    ctx.set_stdin(content);
                }
                StreamTarget::Inherit | StreamTarget::Stderr | StreamTarget::Stdout => {}
            }
        } else if redir.op == RedirectionOp::RedirectHereString {
            let content = expand_word(&redir.target, ctx).await?;
            ctx.set_stdin(format!("{}\n", content).into_bytes());
        } else if matches!(
            redir.op,
            RedirectionOp::RedirectHereDoc | RedirectionOp::RedirectHereDocStrip
        ) {
            let content = expand_word(&redir.target, ctx).await?;
            ctx.set_stdin(content.into_bytes());
        } else if matches!(
            redir.op,
            RedirectionOp::RedirectOut
                | RedirectionOp::RedirectClobber
                | RedirectionOp::RedirectAppend
                | RedirectionOp::RedirectStderr
                | RedirectionOp::RedirectStderrAppend
                | RedirectionOp::RedirectAll
                | RedirectionOp::RedirectAllAppend
        ) {
            let target_path = expand_word(&redir.target, ctx).await?;
            let target = match StreamTarget::from_redirection(redir.op, &target_path, ctx.cwd()) {
                Ok(target) => target,
                Err(error) => {
                    let (_, stderr_destination) = output_destinations(
                        &node.redirections[..index],
                        &expanded_output_targets[..index],
                    );
                    return redirection_failure(
                        error,
                        &command_name,
                        node.span.line,
                        &stderr_destination,
                        ctx,
                    )
                    .await;
                }
            };
            let noclobber = ctx.options().noclobber && redir.op == RedirectionOp::RedirectOut;
            if let Err(error) = target.prepare_for_write(noclobber) {
                let (_, stderr_destination) = output_destinations(
                    &node.redirections[..index],
                    &expanded_output_targets[..index],
                );
                return redirection_failure(
                    error,
                    &command_name,
                    node.span.line,
                    &stderr_destination,
                    ctx,
                )
                .await;
            }
            expanded_output_targets[index] = Some(target);
        }
    }

    if node.raw_name == "=" {
        let status = assignment_only_status.unwrap_or(0);
        ctx.set_last_exit(status);
        return Ok(status);
    }

    // 3. Expand arguments
    let mut expanded_args = Vec::new();
    for arg in &node.args {
        if let Some(items) = expand_word_as_items(arg, ctx).await? {
            expanded_args.extend(items);
            continue;
        }
        let val = expand_word(arg, ctx).await?;
        expanded_args.push(val);
    }

    if ctx.options().xtrace {
        let trace = if expanded_args.is_empty() {
            format!("+ {}\n", command_name)
        } else {
            format!("+ {} {}\n", command_name, expanded_args.join(" "))
        };
        ctx.write_stderr(trace.as_bytes());
    }

    // Check if output redirections are present
    let has_output_redirs = node.redirections.iter().any(|r| {
        r.op != RedirectionOp::RedirectIn
            && r.op != RedirectionOp::RedirectHereString
            && r.op != RedirectionOp::RedirectHereDoc
            && r.op != RedirectionOp::RedirectHereDocStrip
    });

    let (stdout_destination, stderr_destination) =
        output_destinations(&node.redirections, &expanded_output_targets);
    let previous_external_outputs = if has_output_redirs {
        Some(
            ctx.replace_external_file_outputs(distinct_file_destinations(
                &stdout_destination,
                &stderr_destination,
            )),
        )
    } else {
        None
    };
    let (result, captured_out, captured_err) = if has_output_redirs {
        let previous_capture = ctx.begin_redirect_capture(stdout_destination == stderr_destination);
        let res = dispatch_node_target(&target, &command_name, &expanded_args, ctx).await;
        let (out, err) = ctx.finish_redirect_capture(previous_capture);
        (res, Some(out), Some(err))
    } else {
        let res = dispatch_node_target(&target, &command_name, &expanded_args, ctx).await;
        (res, None, None)
    };
    if let Some(previous) = previous_external_outputs {
        ctx.replace_external_file_outputs(previous);
    }

    // 4. Apply output redirections
    if has_output_redirs {
        let out_bytes = captured_out.unwrap_or_default();
        let err_bytes = captured_err.unwrap_or_default();
        if stdout_destination == stderr_destination {
            write_to_destination(&stdout_destination, &out_bytes, ctx)?;
        } else {
            write_to_destination(&stdout_destination, &out_bytes, ctx)?;
            write_to_destination(&stderr_destination, &err_bytes, ctx)?;
        }
    }

    match result {
        Ok(code) => {
            ctx.set_last_exit(code);
            if code != 0 {
                report_command_failure(ctx, &command_name, node.span.line, code, None).await;
            }
            Ok(code)
        }
        Err(e) => {
            let code = e.exit_code();
            report_command_failure(
                ctx,
                &command_name,
                node.span.line,
                code,
                Some(e.to_string()),
            )
            .await;
            Err(e)
        }
    }
}

async fn dispatch_node_target(
    target: &CommandTarget,
    raw_name: &str,
    args: &[String],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    if let Some(func_plan) = ctx.get_function(raw_name) {
        ctx.push_scope();
        ctx.enter_function();
        let previous_args = ctx.positional_args_with_zero().to_vec();
        let mut func_args = vec![previous_args[0].clone()];
        func_args.extend_from_slice(args);
        ctx.set_positional_args(func_args);

        let status = func_plan.execute(ctx).await;
        ctx.set_positional_args(previous_args);
        let return_code = ctx.take_return();
        ctx.exit_function();
        let _ = ctx.pop_scope();
        return match return_code {
            Some(rc) => Ok(rc),
            None => status,
        };
    }

    match target {
        CommandTarget::Builtin(builtin_fn) => builtin_fn(args, ctx).await,

        CommandTarget::UserFunction { name: _, plan } => {
            ctx.push_scope();
            ctx.enter_function();
            let previous_args = ctx.positional_args_with_zero().to_vec();
            let mut func_args = vec![previous_args[0].clone()];
            func_args.extend_from_slice(args);
            ctx.set_positional_args(func_args);

            let status = plan.execute(ctx).await;
            ctx.set_positional_args(previous_args);
            let return_code = ctx.take_return();
            ctx.exit_function();
            let _ = ctx.pop_scope();
            match return_code {
                Some(rc) => Ok(rc),
                None => status,
            }
        }

        CommandTarget::External(_) => execute_external_command(raw_name, args, ctx).await,

        CommandTarget::Dynamic(_) => execute_external_command(raw_name, args, ctx).await,
    }
}

pub(crate) async fn execute_external_command(
    raw_name: &str,
    args: &[String],
    ctx: &mut ExecutionContext,
) -> Result<i32, ExecError> {
    execute_external_command_with_path(raw_name, args, ctx, None).await
}

pub(crate) async fn execute_external_command_with_path(
    raw_name: &str,
    args: &[String],
    ctx: &mut ExecutionContext,
    path_override: Option<&str>,
) -> Result<i32, ExecError> {
    if raw_name.is_empty() {
        ctx.write_stderr(b"esh: : command not found\n");
        return Ok(127);
    }

    let requested_path = std::path::PathBuf::from(raw_name);
    let bin_path = if requested_path.is_absolute() {
        requested_path
    } else {
        ctx.cwd().join(requested_path)
    };
    let path_value = path_override
        .map(str::to_owned)
        .or_else(|| ctx.get_var("PATH"));
    let resolved_bin = if !raw_name.contains('/')
        && !(cfg!(windows) && raw_name.contains('\\'))
        && let Some(path_var) = path_value.as_deref()
    {
        let mut non_executable = None;
        std::env::split_paths(path_var)
            .map(|dir| {
                if dir.is_absolute() {
                    dir.join(raw_name)
                } else {
                    ctx.cwd().join(dir).join(raw_name)
                }
            })
            .find(|candidate| {
                if crate::runtime::builtins::command::is_executable_candidate(candidate) {
                    true
                } else {
                    if non_executable.is_none() && candidate.exists() {
                        non_executable = Some(candidate.clone());
                    }
                    false
                }
            })
            .or(non_executable)
    } else {
        Some(bin_path)
    };
    let Some(resolved_bin) = resolved_bin else {
        ctx.write_stderr(format!("esh: {raw_name}: command not found\n").as_bytes());
        return Ok(127);
    };
    let input_data = ctx.take_stdin();
    let stdin_is_piped = input_data.is_some();
    let capture = ctx.is_capturing();
    let mode = if capture || stdin_is_piped {
        ProcessMode::SupervisedAutomation
    } else {
        ProcessMode::InteractiveForeground
    };
    let resolved_bin_exists = resolved_bin.exists();
    let mut spec = ProcessSpec::new(resolved_bin, ctx.cwd(), mode)
        .args(args.iter().map(String::as_str))
        .environment(EnvironmentPolicy::InheritAndOverride, ctx.env_vars());
    spec.stdin = match input_data {
        Some(bytes) => StdinPolicy::Bytes(bytes),
        None if capture => StdinPolicy::Null,
        None => StdinPolicy::Inherit,
    };
    if let Some(((stdout_path, stdout_append), (stderr_path, stderr_append))) =
        ctx.external_file_outputs()
    {
        spec.stdout = OutputPolicy::File {
            path: stdout_path,
            append: stdout_append,
        };
        spec.stderr = OutputPolicy::File {
            path: stderr_path,
            append: stderr_append,
        };
    } else {
        spec.stdout = if capture {
            OutputPolicy::Capture
        } else {
            OutputPolicy::Inherit
        };
        spec.stderr = if capture && ctx.redirect_capture_merges_streams() {
            OutputPolicy::MergeToStdout
        } else if capture {
            OutputPolicy::Capture
        } else {
            OutputPolicy::Inherit
        };
    }

    let handle = match ProcessHandle::spawn(spec, None) {
        Ok(handle) => handle,
        Err(ProcessError::Spawn { source, .. }) => {
            let status = match source.kind() {
                std::io::ErrorKind::NotFound if !resolved_bin_exists => 127,
                std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied => 126,
                _ => return Err(ExecError::IoError(source.to_string())),
            };
            let diagnostic = if status == 127 {
                format!("esh: {raw_name}: command not found\n")
            } else {
                format!("esh: {raw_name}: permission denied\n")
            };
            ctx.write_stderr(diagnostic.as_bytes());
            return Ok(status);
        }
        Err(error) => return Err(ExecError::IoError(error.to_string())),
    };
    let result = handle
        .wait(ctx.token().clone())
        .await
        .map_err(|error| ExecError::IoError(error.to_string()))?;

    if mode == ProcessMode::InteractiveForeground && result.was_interrupted {
        ctx.cancel();
    }

    if let Some(stdout) = result.stdout {
        ctx.write_stdout(&stdout);
    }
    if let Some(stderr) = result.stderr {
        ctx.write_stderr(&stderr);
    }

    Ok(match result.termination {
        ProcessTermination::Exited(status) => external_process_exit_code(status),
        ProcessTermination::Cancelled => 130,
    })
}

pub async fn expand_word(word: &PlanWord, ctx: &mut ExecutionContext) -> Result<String, ExecError> {
    let mut result = String::new();

    for part in &word.parts {
        match part {
            PlanWordPart::Literal(s) => {
                result.push_str(s);
            }
            PlanWordPart::HomeTilde => {
                if let Some(home) = ctx.get_var("HOME") {
                    result.push_str(&home);
                } else {
                    result.push('~');
                }
            }
            PlanWordPart::SingleQuoted(s) => {
                result.push_str(s);
            }
            PlanWordPart::CaseQuoted(s) => {
                result.push_str(&escape_pattern_literal(s));
            }
            PlanWordPart::CaseDoubleQuoted(s) => {
                let expanded = expand_double_quoted(s, ctx).await?;
                result.push_str(&escape_pattern_literal(&expanded));
            }
            PlanWordPart::Variable(name) => {
                let expanded = expand_parameter_expr(name, ctx).await?;
                result.push_str(&expanded);
            }
            PlanWordPart::DoubleQuoted(content) => {
                let expanded = expand_double_quoted(content, ctx).await?;
                result.push_str(&expanded);
            }
            PlanWordPart::CommandSub(sub_script) => {
                if sub_script.starts_with('(') && sub_script.ends_with(')') {
                    let inner = &sub_script[1..sub_script.len() - 1].trim();
                    let val = crate::runtime::arith::eval_arithmetic_expr(inner, ctx)?;
                    result.push_str(&val.to_string());
                } else {
                    let sub_script = unescape_nested_backticks(sub_script);
                    let plan = compile_script(&sub_script).map_err(|e| {
                        ExecError::Custom(format!("Command substitution parse error: {}", e))
                    })?;
                    let mut sub_ctx = ctx.clone_isolated();
                    sub_ctx.enable_capture();

                    let sub_status = plan.execute(&mut sub_ctx).await?;
                    ctx.record_command_substitution(sub_status);
                    if let Some(err) = sub_ctx.get_captured_stderr_bytes() {
                        ctx.write_stderr(&err);
                    }

                    let stdout = sub_ctx.get_captured_stdout().unwrap_or_default();
                    let trimmed = stdout.trim_end_matches('\n');
                    result.push_str(trimmed);
                }
            }
            PlanWordPart::HereDocBody(body, quoted, strip_tabs) => {
                let body = if *strip_tabs {
                    body.split_inclusive('\n')
                        .map(|line| line.trim_start_matches('\t'))
                        .collect::<String>()
                } else {
                    body.clone()
                };
                if *quoted {
                    result.push_str(&body);
                } else {
                    result.push_str(&expand_double_quoted_mode(&body, ctx, false).await?);
                }
            }
        }
    }

    Ok(result)
}

async fn expand_case_pattern(
    word: &PlanWord,
    ctx: &mut ExecutionContext,
) -> Result<String, ExecError> {
    let mut result = String::new();
    for part in &word.parts {
        match part {
            PlanWordPart::CaseQuoted(value) => {
                result.push_str(&escape_pattern_literal(value));
            }
            PlanWordPart::CaseDoubleQuoted(value) => {
                let expanded = expand_double_quoted(value, ctx).await?;
                result.push_str(&escape_pattern_literal(&expanded));
            }
            _ => {
                let single_part = PlanWord {
                    parts: vec![part.clone()],
                    span: word.span,
                };
                result.push_str(&expand_word(&single_part, ctx).await?);
            }
        }
    }
    Ok(result)
}

fn unescape_nested_backticks(script: &str) -> String {
    let mut result = String::with_capacity(script.len());
    let mut chars = script.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' && chars.peek() == Some(&'`') {
            chars.next();
            result.push('`');
        } else {
            result.push(ch);
        }
    }
    result
}

fn escape_pattern_literal(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        if matches!(ch, '*' | '?' | '[' | ']' | '\\' | '-' | '!' | '^') {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

// Parameter modifier operands have their own quote delimiters while still
// inheriting the enclosing word's double-quoted context.
async fn expand_parameter_operand(
    operand: &str,
    ctx: &mut ExecutionContext,
    outer_double_quoted: bool,
) -> Result<String, ExecError> {
    let bytes = operand.as_bytes();
    let mut result = String::new();
    let mut quote = None;
    let mut segment_start = 0;
    let mut index = 0;

    while index < bytes.len() {
        let current = bytes[index];
        if quote == Some(b'\'') {
            if current == b'\'' {
                result.push_str(&operand[segment_start..index]);
                quote = None;
                index += 1;
                segment_start = index;
            } else {
                let character = operand[index..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary");
                index += character.len_utf8();
            }
            continue;
        }

        if !outer_double_quoted && quote.is_none() && current == b'\\' && index + 1 < bytes.len() {
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..index],
                    ctx,
                    false,
                ))
                .await?,
            );
            let next = bytes[index + 1];
            if next != b'\n' {
                let character = operand[index + 1..]
                    .chars()
                    .next()
                    .expect("valid UTF-8 boundary");
                result.push(character);
                index += 1 + character.len_utf8();
            } else {
                index += 2;
            }
            segment_start = index;
            continue;
        }

        if (quote == Some(b'"') || (quote.is_none() && outer_double_quoted))
            && current == b'\\'
            && index + 1 < bytes.len()
        {
            index += 2;
            continue;
        }

        if quote == Some(b'"') && current == b'"' {
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..index],
                    ctx,
                    true,
                ))
                .await?,
            );
            quote = None;
            index += 1;
            segment_start = index;
            continue;
        }

        if quote.is_none() && (current == b'\'' || current == b'"') {
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..index],
                    ctx,
                    outer_double_quoted,
                ))
                .await?,
            );
            quote = Some(current);
            index += 1;
            segment_start = index;
            continue;
        }

        if current == b'$' && bytes.get(index + 1) == Some(&b'{') {
            let in_double_quotes = outer_double_quoted || quote == Some(b'"');
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..index],
                    ctx,
                    in_double_quotes,
                ))
                .await?,
            );

            let mut cursor = index + 2;
            let mut depth = 1usize;
            let mut nested_quote = None;
            while cursor < bytes.len() && depth > 0 {
                let part = bytes[cursor];
                if let Some(active_quote) = nested_quote {
                    if part == b'\\' && active_quote == b'"' {
                        cursor = (cursor + 2).min(bytes.len());
                    } else if part == active_quote {
                        nested_quote = None;
                        cursor += 1;
                    } else if part == b'$' && bytes.get(cursor + 1) == Some(&b'{') {
                        depth += 1;
                        cursor += 2;
                    } else {
                        cursor += 1;
                    }
                } else if part == b'\'' || part == b'"' {
                    nested_quote = Some(part);
                    cursor += 1;
                } else if part == b'\\' {
                    cursor = (cursor + 2).min(bytes.len());
                } else if part == b'$' && bytes.get(cursor + 1) == Some(&b'{') {
                    depth += 1;
                    cursor += 2;
                } else if part == b'}' {
                    depth -= 1;
                    if depth > 0 {
                        cursor += 1;
                    }
                } else {
                    cursor += 1;
                }
            }

            if depth == 0 {
                result.push_str(
                    &Box::pin(expand_parameter_expr_mode(
                        &operand[index + 2..cursor],
                        ctx,
                        in_double_quotes,
                    ))
                    .await?,
                );
                index = cursor + 1;
                segment_start = index;
                continue;
            }
            index += 2;
            segment_start = index;
            continue;
        }

        if current == b'`' {
            index += 1;
            while index < bytes.len() {
                if bytes[index] == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if bytes[index] == b'`' {
                    index += 1;
                    break;
                } else {
                    let character = operand[index..]
                        .chars()
                        .next()
                        .expect("valid UTF-8 boundary");
                    index += character.len_utf8();
                }
            }
            continue;
        }

        if current == b'$' && bytes.get(index + 1) == Some(&b'(') {
            index += 2;
            let mut depth = 1usize;
            let mut command_quote = None;
            while index < bytes.len() && depth > 0 {
                let part = bytes[index];
                if part == b'\\' {
                    index = (index + 2).min(bytes.len());
                } else if let Some(active_quote) = command_quote {
                    if part == active_quote {
                        command_quote = None;
                    }
                    index += 1;
                } else if part == b'\'' || part == b'"' {
                    command_quote = Some(part);
                    index += 1;
                } else if part == b'(' {
                    depth += 1;
                    index += 1;
                } else if part == b')' {
                    depth -= 1;
                    index += 1;
                } else {
                    let character = operand[index..]
                        .chars()
                        .next()
                        .expect("valid UTF-8 boundary");
                    index += character.len_utf8();
                }
            }
            continue;
        }

        let character = operand[index..]
            .chars()
            .next()
            .expect("valid UTF-8 boundary");
        index += character.len_utf8();
    }

    match quote {
        Some(b'\'') => result.push_str(&operand[segment_start..]),
        Some(b'"') => {
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..],
                    ctx,
                    true,
                ))
                .await?,
            );
        }
        _ => {
            result.push_str(
                &Box::pin(expand_double_quoted_mode(
                    &operand[segment_start..],
                    ctx,
                    outer_double_quoted,
                ))
                .await?,
            );
        }
    }

    Ok(result)
}

pub async fn expand_parameter_expr(
    expr: &str,
    ctx: &mut ExecutionContext,
) -> Result<String, ExecError> {
    expand_parameter_expr_mode(expr, ctx, false).await
}

async fn expand_parameter_expr_mode(
    expr: &str,
    ctx: &mut ExecutionContext,
    in_double_quotes: bool,
) -> Result<String, ExecError> {
    // 0. ${!VAR[@]} or ${!VAR[*]} -> list of keys / indices
    if let Some(rest) = expr.strip_prefix('!')
        && let Some((var_name, sub)) = rest.split_once('[')
    {
        let sub_clean = sub.trim_end_matches(']');
        if sub_clean == "@" || sub_clean == "*" {
            if let Some(val) = ctx.get_shell_val(var_name) {
                return Ok(val.get_keys().join(" "));
            }
            return Ok(String::new());
        }
    }

    // The special parameter `#` is ambiguous with the string-length prefix.
    // Recognize its operator forms before parsing `${#parameter}`.
    if let Some(rest) = expr.strip_prefix('#') {
        let (operator, operand) = if let Some(operand) = rest.strip_prefix(":-") {
            (Some(":-"), operand)
        } else if let Some(operand) = rest.strip_prefix('-') {
            (Some("-"), operand)
        } else if let Some(operand) = rest.strip_prefix(":+") {
            (Some(":+"), operand)
        } else if let Some(operand) = rest.strip_prefix('+') {
            (Some("+"), operand)
        } else if let Some(operand) = rest.strip_prefix(":?") {
            (Some(":?"), operand)
        } else if let Some(operand) = rest.strip_prefix('?') {
            (Some("?"), operand)
        } else if rest.starts_with(":=") || rest.starts_with('=') {
            return Err(ExecError::Custom(
                "invalid variable name in assignment expansion: #".to_string(),
            ));
        } else {
            (None, "")
        };

        if let Some(operator) = operator {
            let count = ctx.get_var("#").unwrap_or_default();
            return match operator {
                ":+" | "+" => {
                    Box::pin(expand_parameter_operand(operand, ctx, in_double_quotes)).await
                }
                _ => Ok(count),
            };
        }
    }

    // 1. ${#VAR} / ${#VAR[@]} / ${#VAR[idx]} -> string length, element count, or positional parameter count
    if let Some(var_name) = expr.strip_prefix('#') {
        if let Some((name, sub)) = var_name.split_once('[') {
            let sub_clean = sub.trim_end_matches(']');
            if sub_clean == "@" || sub_clean == "*" {
                if let Some(val) = ctx.get_shell_val(name) {
                    return Ok(val.len().to_string());
                }
                if ctx.get_var(name).is_none() && ctx.options().nounset {
                    return Err(ExecError::Custom(format!("{}: unbound variable", name)));
                }
                return Ok("0".to_string());
            } else {
                if let Some(val) = ctx.get_shell_val(name)
                    && let Some(item) = val.get_subscript(sub_clean)
                {
                    return Ok(item.chars().count().to_string());
                }
                if ctx.get_var(name).is_none() && ctx.options().nounset {
                    return Err(ExecError::Custom(format!("{}: unbound variable", name)));
                }
                return Ok("0".to_string());
            }
        }
        if var_name == "@" || var_name == "*" {
            return Ok(ctx.positional_args().len().to_string());
        }
        let val = match ctx.get_var(var_name) {
            Some(value) => value,
            None if ctx.options().nounset => {
                return Err(ExecError::Custom(format!("{}: unbound variable", var_name)));
            }
            None => String::new(),
        };
        return Ok(val.chars().count().to_string());
    }

    // Pattern removal on positional parameters is unspecified by POSIX. ESH
    // follows the useful per-parameter behavior also used by Bash: `@` keeps
    // a list when expanded as a standalone word, while `*` joins the removed
    // values with ESH's literal-space separator.
    for parameter in ["@", "*"] {
        if let Some(pattern_expression) = expr.strip_prefix(parameter)
            && let Some((removal, pattern_expression)) =
                parse_parameter_pattern_removal(pattern_expression)
        {
            let pattern = expand_parameter_pattern(pattern_expression, ctx).await?;
            let values = ctx
                .positional_args()
                .iter()
                .map(|value| apply_parameter_pattern_removal(value, &pattern, removal))
                .collect::<Vec<_>>();
            return Ok(values.join(" "));
        }
    }

    // 1.5 Subscript Access: ${VAR[sub]} or ${VAR[sub]:-default} or ${VAR[sub]##pattern}
    if let Some((var_name, sub_part)) = expr.split_once('[')
        && is_valid_var_ident(var_name)
        && let Some((sub_spec, modifier)) = sub_part.split_once(']')
    {
        let resolved_sub =
            Box::pin(expand_parameter_operand(sub_spec, ctx, in_double_quotes)).await?;
        let sub_clean = resolved_sub.as_str();
        let current_value = if let Some(val) = ctx.get_shell_val(var_name) {
            val.get_subscript(sub_clean)
        } else {
            None
        };
        let val_str = current_value.clone().unwrap_or_default();

        if modifier.is_empty() {
            if matches!(sub_clean, "@" | "*") && ctx.get_shell_val(var_name).is_none() {
                return Ok(String::new());
            }
            if current_value.is_none() && ctx.options().nounset {
                return Err(ExecError::Custom(format!("{}: unbound variable", var_name)));
            }
            return Ok(val_str);
        }

        if let Some(default_val) = modifier.strip_prefix(":-") {
            if current_value
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            {
                return Ok(val_str);
            }
            return Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await;
        }
        if let Some(default_val) = modifier.strip_prefix('-') {
            if let Some(value) = current_value {
                return Ok(value);
            }
            return Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await;
        }
        if let Some(default_val) = modifier.strip_prefix(":=") {
            if current_value
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            {
                return Ok(val_str);
            }
            let value =
                Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await?;
            set_parameter_subscript(ctx, var_name, sub_clean, value.clone());
            return Ok(value);
        }
        if let Some(default_val) = modifier.strip_prefix('=') {
            if let Some(value) = current_value {
                return Ok(value);
            }
            let value =
                Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await?;
            set_parameter_subscript(ctx, var_name, sub_clean, value.clone());
            return Ok(value);
        }
        if let Some(message) = modifier.strip_prefix(":?") {
            if current_value
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            {
                return Ok(val_str);
            }
            let parameter = format!("{}[{}]", var_name, sub_clean);
            let message = if message.is_empty() {
                format!("parameter '{}' is null or unset", parameter)
            } else {
                Box::pin(expand_parameter_operand(message, ctx, in_double_quotes)).await?
            };
            return Err(ExecError::Custom(message));
        }
        if let Some(message) = modifier.strip_prefix('?') {
            if let Some(value) = current_value {
                return Ok(value);
            }
            let parameter = format!("{}[{}]", var_name, sub_clean);
            let message = if message.is_empty() {
                format!("parameter '{}' is unset", parameter)
            } else {
                Box::pin(expand_parameter_operand(message, ctx, in_double_quotes)).await?
            };
            return Err(ExecError::Custom(message));
        }
        if let Some(alternate) = modifier.strip_prefix(":+") {
            if current_value
                .as_ref()
                .is_some_and(|value| !value.is_empty())
            {
                return Box::pin(expand_parameter_operand(alternate, ctx, in_double_quotes)).await;
            }
            return Ok(String::new());
        }
        if let Some(alternate) = modifier.strip_prefix('+') {
            if current_value.is_some() {
                return Box::pin(expand_parameter_operand(alternate, ctx, in_double_quotes)).await;
            }
            return Ok(String::new());
        }
        if (modifier.starts_with('#') || modifier.starts_with('%') || modifier.starts_with(':'))
            && current_value.is_none()
            && ctx.options().nounset
        {
            return Err(ExecError::Custom(format!(
                "{}[{}]: unbound variable",
                var_name, sub_clean
            )));
        }
        if matches!(sub_clean, "@" | "*")
            && let Some((removal, pattern_expression)) = parse_parameter_pattern_removal(modifier)
        {
            let Some(values) = ctx.get_shell_val(var_name).map(|value| value.get_values()) else {
                if ctx.options().nounset {
                    return Err(ExecError::Custom(format!(
                        "{}[{}]: unbound variable",
                        var_name, sub_clean
                    )));
                }
                return Ok(String::new());
            };
            let pattern = expand_parameter_pattern(pattern_expression, ctx).await?;
            return Ok(values
                .iter()
                .map(|value| apply_parameter_pattern_removal(value, &pattern, removal))
                .collect::<Vec<_>>()
                .join(" "));
        }
        if matches!(sub_clean, "@" | "*")
            && let Some(slice_spec) = modifier.strip_prefix(':')
        {
            let Some(value) = ctx.get_shell_val(var_name) else {
                return Ok(String::new());
            };
            return Ok(slice_parameter_array_items(value, slice_spec)?.join(" "));
        }
        if let Some(pattern) = modifier.strip_prefix("##") {
            let pattern = expand_parameter_pattern(pattern, ctx).await?;
            return Ok(strip_prefix_pattern(&val_str, &pattern, true));
        }
        if let Some(pattern) = modifier.strip_prefix('#') {
            let pattern = expand_parameter_pattern(pattern, ctx).await?;
            return Ok(strip_prefix_pattern(&val_str, &pattern, false));
        }
        if let Some(pattern) = modifier.strip_prefix("%%") {
            let pattern = expand_parameter_pattern(pattern, ctx).await?;
            return Ok(strip_suffix_pattern(&val_str, &pattern, true));
        }
        if let Some(pattern) = modifier.strip_prefix('%') {
            let pattern = expand_parameter_pattern(pattern, ctx).await?;
            return Ok(strip_suffix_pattern(&val_str, &pattern, false));
        }
        if let Some(slice_spec) = modifier.strip_prefix(':') {
            return slice_parameter_value(&val_str, slice_spec);
        }
        return Ok(val_str);
    }

    // Recognize replacement before default operators because its pattern and
    // replacement operands may themselves contain nested parameter operators.
    if let Some((var_name, rest, replace_all)) = parse_parameter_replacement_expression(expr) {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        let (pattern, replacement) = split_parameter_replacement(rest);
        let pattern = Box::pin(expand_parameter_operand(&pattern, ctx, in_double_quotes)).await?;
        let replacement = Box::pin(expand_parameter_operand(
            &replacement,
            ctx,
            in_double_quotes,
        ))
        .await?;
        if pattern.is_empty() {
            return Ok(val);
        }
        return Ok(if replace_all {
            val.replace(&pattern, &replacement)
        } else {
            val.replacen(&pattern, &replacement, 1)
        });
    }

    // Pattern-removal operators must be recognized before default operators:
    // their pattern operands may contain nested `${...}` expansions that use
    // `:-`, `:=`, and related syntax themselves.
    if let Some((var_name, pattern)) = expr.split_once("##")
        && is_valid_var_ident(var_name)
    {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        let pattern = expand_parameter_pattern(pattern, ctx).await?;
        return Ok(strip_prefix_pattern(&val, &pattern, true));
    }
    if let Some((var_name, pattern)) = expr.split_once('#')
        && is_valid_var_ident(var_name)
    {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        let pattern = expand_parameter_pattern(pattern, ctx).await?;
        return Ok(strip_prefix_pattern(&val, &pattern, false));
    }
    if let Some((var_name, pattern)) = expr.split_once("%%")
        && is_valid_var_ident(var_name)
    {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        let pattern = expand_parameter_pattern(pattern, ctx).await?;
        return Ok(strip_suffix_pattern(&val, &pattern, true));
    }
    if let Some((var_name, pattern)) = expr.split_once('%')
        && is_valid_var_ident(var_name)
    {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        let pattern = expand_parameter_pattern(pattern, ctx).await?;
        return Ok(strip_suffix_pattern(&val, &pattern, false));
    }

    // 2. ${VAR:-default}
    if let Some((name, default_val)) = expr.split_once(":-") {
        ensure_parameter_name(name)?;
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        return Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await;
    }

    // 3. ${VAR:=default}
    if let Some((name, default_val)) = expr.split_once(":=") {
        if !is_valid_var_ident(name) {
            return Err(ExecError::Custom(format!(
                "invalid variable name in assignment expansion: {name}"
            )));
        }
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        let evaluated_default =
            Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await?;
        ctx.set_scoped_var(name, evaluated_default.clone());
        return Ok(evaluated_default);
    }

    // 4. ${VAR:?error_msg}
    if let Some((name, err_msg)) = expr.split_once(":?") {
        ensure_parameter_name(name)?;
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        let evaluated_msg = if err_msg.is_empty() {
            format!("parameter '{}' is null or unset", name)
        } else {
            Box::pin(expand_parameter_operand(err_msg, ctx, in_double_quotes)).await?
        };
        return Err(ExecError::Custom(evaluated_msg));
    }

    // 5. ${VAR:+alternate}
    if let Some((name, alt_val)) = expr.split_once(":+") {
        ensure_parameter_name(name)?;
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Box::pin(expand_parameter_operand(alt_val, ctx, in_double_quotes)).await;
        }
        return Ok(String::new());
    }

    // POSIX-style forms without `:` distinguish unset from set-but-empty.
    if let Some((name, default_val)) = expr.split_once('-')
        && is_parameter_name(name)
    {
        if parameter_is_set(name, ctx) {
            return Ok(ctx.get_var(name).unwrap_or_default());
        }
        return Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await;
    }
    if let Some((name, default_val)) = expr.split_once('=')
        && is_valid_var_ident(name)
    {
        if let Some(value) = ctx.get_var(name) {
            return Ok(value);
        }
        let evaluated_default =
            Box::pin(expand_parameter_operand(default_val, ctx, in_double_quotes)).await?;
        ctx.set_scoped_var(name, evaluated_default.clone());
        return Ok(evaluated_default);
    }
    if let Some((name, err_msg)) = expr.split_once('?')
        && is_parameter_name(name)
    {
        if parameter_is_set(name, ctx) {
            return Ok(ctx.get_var(name).unwrap_or_default());
        }
        let message = if err_msg.is_empty() {
            format!("parameter '{}' is unset", name)
        } else {
            Box::pin(expand_parameter_operand(err_msg, ctx, in_double_quotes)).await?
        };
        return Err(ExecError::Custom(message));
    }
    if let Some((name, alt_val)) = expr.split_once('+')
        && is_parameter_name(name)
    {
        if parameter_is_set(name, ctx) {
            return Box::pin(expand_parameter_operand(alt_val, ctx, in_double_quotes)).await;
        }
        return Ok(String::new());
    }

    // 6. Substring Slicing: ${VAR:offset:length} or ${VAR:offset}
    if let Some((var_name, slice_spec)) = expr.split_once(':')
        && is_valid_var_ident(var_name)
    {
        let val = get_parameter_for_modifier(ctx, var_name)?;
        return slice_parameter_value(&val, slice_spec);
    }

    // 10. Simple variable lookup
    if let Some(val) = ctx.get_var(expr) {
        Ok(val)
    } else if ctx.options().nounset {
        Err(ExecError::Custom(format!("{}: unbound variable", expr)))
    } else {
        Ok(String::new())
    }
}

fn set_parameter_subscript(ctx: &mut ExecutionContext, name: &str, subscript: &str, value: String) {
    let is_map = matches!(
        ctx.get_shell_val(name),
        Some(crate::runtime::context::ShellValue::Map(_))
    );
    if !is_map && let Ok(index) = subscript.parse::<usize>() {
        ctx.set_array_elem(name, index, value);
    } else {
        ctx.set_map_elem(name, subscript.to_string(), value);
    }
}

fn parse_parameter_replacement_expression(expr: &str) -> Option<(&str, &str, bool)> {
    let name_end = expr
        .char_indices()
        .find_map(|(index, ch)| (!ch.is_ascii_alphanumeric() && ch != '_').then_some(index))
        .unwrap_or(expr.len());
    let name = &expr[..name_end];
    if !is_valid_var_ident(name) {
        return None;
    }

    let modifier = &expr[name_end..];
    if let Some(rest) = modifier.strip_prefix("//") {
        Some((name, rest, true))
    } else {
        modifier.strip_prefix('/').map(|rest| (name, rest, false))
    }
}

fn split_parameter_replacement(rest: &str) -> (String, String) {
    let mut pattern = String::new();
    let mut replacement = String::new();
    let mut in_replacement = false;
    let mut chars = rest.chars().peekable();
    let mut brace_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut quote = None;
    let mut in_backticks = false;

    while let Some(ch) = chars.next() {
        let output = if in_replacement {
            &mut replacement
        } else {
            &mut pattern
        };

        if ch == '\\' {
            if chars.peek() == Some(&'/') {
                chars.next();
                output.push('/');
            } else {
                output.push(ch);
                if let Some(escaped) = chars.next() {
                    output.push(escaped);
                }
            }
            continue;
        }

        if in_backticks {
            output.push(ch);
            if ch == '`' {
                in_backticks = false;
            }
            continue;
        }

        if let Some(active_quote) = quote {
            output.push(ch);
            if ch == active_quote {
                quote = None;
            } else if ch == '$' {
                if chars.peek() == Some(&'{') {
                    output.push(chars.next().expect("peeked opening brace"));
                    brace_depth += 1;
                } else if chars.peek() == Some(&'(') {
                    output.push(chars.next().expect("peeked opening parenthesis"));
                    paren_depth += 1;
                }
            } else if ch == '}' && brace_depth > 0 {
                brace_depth -= 1;
            }
            continue;
        }

        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            output.push(ch);
        } else if ch == '`' {
            in_backticks = true;
            output.push(ch);
        } else if ch == '$' && chars.peek() == Some(&'{') {
            output.push(ch);
            output.push(chars.next().expect("peeked opening brace"));
            brace_depth += 1;
        } else if ch == '$' && chars.peek() == Some(&'(') {
            output.push(ch);
            output.push(chars.next().expect("peeked opening parenthesis"));
            paren_depth += 1;
        } else if ch == '}' && brace_depth > 0 {
            brace_depth -= 1;
            output.push(ch);
        } else if ch == '(' && paren_depth > 0 {
            paren_depth += 1;
            output.push(ch);
        } else if ch == ')' && paren_depth > 0 {
            paren_depth -= 1;
            output.push(ch);
        } else if ch == '/' && brace_depth == 0 && paren_depth == 0 {
            if in_replacement {
                output.push(ch);
            } else {
                in_replacement = true;
            }
        } else {
            output.push(ch);
        }
    }

    (pattern, replacement)
}

fn get_parameter_for_modifier(ctx: &ExecutionContext, name: &str) -> Result<String, ExecError> {
    match ctx.get_var(name) {
        Some(value) => Ok(value),
        None if ctx.options().nounset => {
            Err(ExecError::Custom(format!("{name}: unbound variable")))
        }
        None => Ok(String::new()),
    }
}

fn slice_parameter_value(value: &str, slice_spec: &str) -> Result<String, ExecError> {
    let (offset, length) = parse_slice_spec(slice_spec)?;
    let chars = value.chars().collect::<Vec<_>>();
    let start = offset.min(chars.len());
    let end = length
        .map(|length| start.saturating_add(length).min(chars.len()))
        .unwrap_or(chars.len());
    Ok(chars[start..end].iter().collect())
}

fn slice_parameter_array_items(
    value: &crate::runtime::context::ShellValue,
    slice_spec: &str,
) -> Result<Vec<String>, ExecError> {
    let (offset, length) = parse_slice_spec(slice_spec)?;
    match value {
        crate::runtime::context::ShellValue::String(value) => {
            Ok(vec![slice_parameter_value(value, slice_spec)?])
        }
        crate::runtime::context::ShellValue::Array(array) => Ok(array
            .range(offset..)
            .map(|(_, value)| value.clone())
            .take(length.unwrap_or(usize::MAX))
            .collect()),
        crate::runtime::context::ShellValue::Map(_) => Ok(value
            .get_values()
            .into_iter()
            .skip(offset)
            .take(length.unwrap_or(usize::MAX))
            .collect()),
    }
}

fn parse_slice_spec(slice_spec: &str) -> Result<(usize, Option<usize>), ExecError> {
    let mut parts = slice_spec.split(':');
    let offset = parts
        .next()
        .and_then(parse_saturating_decimal)
        .ok_or_else(|| ExecError::Custom(format!("invalid slice offset: {slice_spec}")))?;
    let length = parts
        .next()
        .map(|part| {
            parse_saturating_decimal(part)
                .ok_or_else(|| ExecError::Custom(format!("invalid slice length: {part}")))
        })
        .transpose()?;
    if parts.next().is_some() {
        return Err(ExecError::Custom(format!(
            "invalid slice specification: {slice_spec}"
        )));
    }
    Ok((offset, length))
}

fn parse_saturating_decimal(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }

    Some(value.bytes().fold(0usize, |number, digit| {
        number
            .saturating_mul(10)
            .saturating_add(usize::from(digit - b'0'))
    }))
}

fn is_valid_var_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('A'..='Z' | 'a'..='z' | '_'))
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_parameter_name(s: &str) -> bool {
    is_valid_var_ident(s)
        || (!s.is_empty() && s.bytes().all(|byte| byte.is_ascii_digit()))
        || matches!(s, "?" | "#" | "@" | "*" | "-")
}

fn parameter_is_set(name: &str, ctx: &ExecutionContext) -> bool {
    if matches!(name, "@" | "*") {
        !ctx.positional_args().is_empty()
    } else {
        ctx.get_var(name).is_some()
    }
}

fn ensure_parameter_name(name: &str) -> Result<(), ExecError> {
    if is_parameter_name(name) {
        Ok(())
    } else {
        Err(ExecError::Custom(format!(
            "invalid parameter name in expansion: {name}"
        )))
    }
}

fn strip_prefix_pattern(text: &str, pattern: &str, greedy: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let tokens = parse_shell_pattern(pattern);
    let matches = shell_pattern_prefix_matches(&tokens, &chars);
    let end = if greedy {
        (0..=chars.len()).rev().find(|&index| matches[index])
    } else {
        (0..=chars.len()).find(|&index| matches[index])
    };
    end.map_or_else(|| text.to_string(), |end| chars[end..].iter().collect())
}

fn strip_suffix_pattern(text: &str, pattern: &str, greedy: bool) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut reversed_chars = chars.clone();
    reversed_chars.reverse();
    let mut reversed_tokens = parse_shell_pattern(pattern);
    reversed_tokens.reverse();
    let matches = shell_pattern_prefix_matches(&reversed_tokens, &reversed_chars);
    let suffix_len = if greedy {
        (0..=chars.len()).rev().find(|&index| matches[index])
    } else {
        (0..=chars.len()).find(|&index| matches[index])
    };
    suffix_len.map_or_else(
        || text.to_string(),
        |suffix_len| chars[..chars.len() - suffix_len].iter().collect(),
    )
}

pub async fn expand_double_quoted(
    content: &str,
    ctx: &mut ExecutionContext,
) -> Result<String, ExecError> {
    expand_double_quoted_mode(content, ctx, true).await
}

async fn expand_double_quoted_mode(
    content: &str,
    ctx: &mut ExecutionContext,
    in_double_quotes: bool,
) -> Result<String, ExecError> {
    let mut res = String::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'\n' {
                i += 2;
                continue;
            }
            if next == b'\r' && bytes.get(i + 2) == Some(&b'\n') {
                i += 3;
                continue;
            }
            if next == b'$' || next == b'\\' || next == b'`' || (in_double_quotes && next == b'"') {
                res.push(next as char);
                i += 2;
                continue;
            }
        }

        if b == b'`' {
            i += 1;
            let start = i;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 1;
                    if i < bytes.len() {
                        i += 1;
                    }
                } else if bytes[i] == b'`' {
                    break;
                } else {
                    i += 1;
                }
            }
            if i == bytes.len() {
                return Err(ExecError::Custom(
                    "Unterminated command substitution using backticks".to_string(),
                ));
            }
            let sub_script = unescape_nested_backticks(&content[start..i]);
            i += 1;
            let plan = compile_script(&sub_script).map_err(|error| {
                ExecError::Custom(format!("Command substitution parse error: {error}"))
            })?;
            let mut sub_ctx = ctx.clone_isolated();
            sub_ctx.enable_capture();
            let sub_status = plan.execute(&mut sub_ctx).await?;
            ctx.record_command_substitution(sub_status);
            if let Some(err) = sub_ctx.get_captured_stderr_bytes() {
                ctx.write_stderr(&err);
            }
            let stdout = sub_ctx.get_captured_stdout().unwrap_or_default();
            res.push_str(stdout.trim_end_matches('\n'));
            continue;
        }

        if b == b'$' {
            i += 1;
            if i >= bytes.len() {
                res.push('$');
                break;
            }

            // Command substitution: $(...) or Arithmetic: $((...))
            if bytes[i] == b'(' {
                i += 1;
                let start = i;
                let mut depth = 1;
                while i < bytes.len() && depth > 0 {
                    let c = bytes[i];
                    if c == b'\\' {
                        i += 2;
                        continue;
                    }
                    if c == b'\'' {
                        i += 1;
                        while i < bytes.len() && bytes[i] != b'\'' {
                            i += 1;
                        }
                        if i < bytes.len() {
                            i += 1;
                        }
                        continue;
                    }
                    if c == b'"' {
                        i += 1;
                        while i < bytes.len() && bytes[i] != b'"' {
                            if bytes[i] == b'\\' {
                                i += 1;
                            }
                            i += 1;
                        }
                        if i < bytes.len() {
                            i += 1;
                        }
                        continue;
                    }
                    if c == b'(' {
                        depth += 1;
                    } else if c == b')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    i += 1;
                }
                let sub_script = &content[start..i];
                if i < bytes.len() && bytes[i] == b')' {
                    i += 1;
                }

                if sub_script.starts_with('(') && sub_script.ends_with(')') {
                    let inner = &sub_script[1..sub_script.len() - 1].trim();
                    let val = crate::runtime::arith::eval_arithmetic_expr(inner, ctx)?;
                    res.push_str(&val.to_string());
                    continue;
                }

                let plan = compile_script(sub_script).map_err(|e| {
                    ExecError::Custom(format!("Command substitution parse error: {}", e))
                })?;
                let mut sub_ctx = ctx.clone_isolated();
                sub_ctx.enable_capture();

                let sub_status = plan.execute(&mut sub_ctx).await?;
                ctx.record_command_substitution(sub_status);
                if let Some(err) = sub_ctx.get_captured_stderr_bytes() {
                    ctx.write_stderr(&err);
                }

                let stdout = sub_ctx.get_captured_stdout().unwrap_or_default();
                let trimmed = stdout.trim_end_matches('\n');
                res.push_str(trimmed);
                continue;
            }

            // ${VAR} or ${VAR:-default}
            if bytes[i] == b'{' {
                i += 1;
                let start = i;
                let mut depth = 1usize;
                let mut quote = None;
                while i < bytes.len() && depth > 0 {
                    let current = bytes[i];
                    if let Some(active_quote) = quote {
                        if current == b'\\' && active_quote == b'"' {
                            i += 1;
                            if i < bytes.len() {
                                i += 1;
                            }
                        } else if current == active_quote {
                            quote = None;
                            i += 1;
                        } else if current == b'$' && bytes.get(i + 1) == Some(&b'{') {
                            depth += 1;
                            i += 2;
                        } else {
                            i += 1;
                        }
                    } else if current == b'\'' || current == b'"' {
                        quote = Some(current);
                        i += 1;
                    } else if current == b'\\' {
                        i += 1;
                        if i < bytes.len() {
                            i += 1;
                        }
                    } else if current == b'$' && bytes.get(i + 1) == Some(&b'{') {
                        depth += 1;
                        i += 2;
                    } else if current == b'}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        i += 1;
                    } else {
                        i += 1;
                    }
                }
                let expr = &content[start..i];
                if i < bytes.len() && bytes[i] == b'}' {
                    i += 1;
                }
                let expanded =
                    Box::pin(expand_parameter_expr_mode(expr, ctx, in_double_quotes)).await?;
                res.push_str(&expanded);
                continue;
            }

            // Special single-character variables: $?, $@, $*, $#, $-, $0..$9
            if bytes[i] == b'?'
                || bytes[i] == b'@'
                || bytes[i] == b'*'
                || bytes[i] == b'#'
                || bytes[i] == b'-'
                || bytes[i].is_ascii_digit()
            {
                let name = &content[i..i + 1];
                i += 1;
                res.push_str(&get_parameter_for_modifier(ctx, name)?);
                continue;
            }

            // Standard identifier: $VAR
            let start = i;
            while i < bytes.len()
                && ((bytes[i] as char).is_ascii_alphanumeric() || bytes[i] == b'_')
            {
                i += 1;
            }
            if start == i {
                res.push('$');
            } else {
                let name = &content[start..i];
                res.push_str(&get_parameter_for_modifier(ctx, name)?);
            }
            continue;
        }

        let character = content[i..]
            .chars()
            .next()
            .expect("byte index is within the UTF-8 string");
        res.push(character);
        i += character.len_utf8();
    }

    Ok(res)
}

fn shell_pattern_matches(pattern: &str, text: &str) -> bool {
    let tokens = parse_shell_pattern(pattern);
    let text: Vec<char> = text.chars().collect();
    shell_pattern_prefix_matches(&tokens, &text)[text.len()]
}

async fn expand_parameter_pattern(
    pattern: &str,
    ctx: &mut ExecutionContext,
) -> Result<String, ExecError> {
    let mut chars = pattern.chars().peekable();
    let mut single_quoted = false;
    let mut double_quoted = false;
    let mut expanded = String::with_capacity(pattern.len());

    while let Some(ch) = chars.next() {
        if !double_quoted && ch == '\'' {
            single_quoted = !single_quoted;
            continue;
        }
        if !single_quoted && ch == '"' {
            double_quoted = !double_quoted;
            continue;
        }
        if !single_quoted && ch == '\\' {
            if let Some(escaped) = chars.next() {
                if escaped == '\n' {
                    continue;
                }
                if !double_quoted || matches!(escaped, '$' | '`' | '"' | '\\') {
                    push_quoted_pattern_char(&mut expanded, escaped);
                } else {
                    expanded.push('\\');
                    expanded.push(escaped);
                }
            } else {
                expanded.push_str("\\\\");
            }
            continue;
        }

        if !single_quoted && ch == '$' {
            let Some(next) = chars.peek().copied() else {
                expanded.push('$');
                continue;
            };
            let value = if next == '(' {
                chars.next();
                let mut substitution = String::from("$(");
                let mut depth = 1usize;
                let mut quote = None;
                while let Some(part) = chars.next() {
                    substitution.push(part);
                    if part == '\\' {
                        if let Some(escaped) = chars.next() {
                            substitution.push(escaped);
                        }
                        continue;
                    }
                    if let Some(active_quote) = quote {
                        if part == active_quote {
                            quote = None;
                        }
                        continue;
                    }
                    if part == '\'' || part == '"' {
                        quote = Some(part);
                    } else if part == '(' {
                        depth += 1;
                    } else if part == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                }
                Box::pin(expand_double_quoted(&substitution, ctx)).await?
            } else if next == '{' {
                chars.next();
                let mut expression = String::new();
                let mut depth = 1usize;
                while let Some(part) = chars.next() {
                    if part == '$' && chars.peek() == Some(&'{') {
                        expression.push(part);
                        expression.push(chars.next().expect("peeked opening brace"));
                        depth += 1;
                    } else if part == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        expression.push(part);
                    } else {
                        expression.push(part);
                    }
                }
                Box::pin(expand_parameter_expr_mode(&expression, ctx, double_quoted)).await?
            } else if next == '?'
                || next == '@'
                || next == '*'
                || next == '#'
                || next == '-'
                || next.is_ascii_digit()
            {
                chars.next();
                get_parameter_for_modifier(ctx, &next.to_string())?
            } else if next == '_' || next.is_ascii_alphabetic() {
                let mut name = String::new();
                while chars
                    .peek()
                    .is_some_and(|part| *part == '_' || part.is_ascii_alphanumeric())
                {
                    name.push(chars.next().expect("peeked identifier character"));
                }
                get_parameter_for_modifier(ctx, &name)?
            } else {
                expanded.push('$');
                continue;
            };

            if single_quoted || double_quoted {
                expanded.push_str(&escape_pattern_literal(&value));
            } else {
                expanded.push_str(&value);
            }
            continue;
        }

        if single_quoted || double_quoted {
            push_quoted_pattern_char(&mut expanded, ch);
        } else {
            expanded.push(ch);
        }
    }

    Ok(expanded)
}

fn push_quoted_pattern_char(pattern: &mut String, ch: char) {
    if matches!(ch, '*' | '?' | '[' | ']' | '\\' | '-' | '!' | '^') {
        pattern.push('\\');
    }
    pattern.push(ch);
}

fn shell_pattern_prefix_matches(tokens: &[ShellPatternToken], text: &[char]) -> Vec<bool> {
    let mut states = vec![false; text.len() + 1];
    states[0] = true;

    for token in tokens {
        let mut next = vec![false; text.len() + 1];
        match token {
            ShellPatternToken::Star => {
                let mut reachable = false;
                for index in 0..=text.len() {
                    reachable |= states[index];
                    next[index] = reachable;
                }
            }
            matcher => {
                for index in 0..text.len() {
                    if states[index] && matcher.matches(text[index]) {
                        next[index + 1] = true;
                    }
                }
            }
        }
        states = next;
    }
    states
}

#[derive(Clone)]
enum ShellPatternToken {
    Literal(char),
    Any,
    Star,
    Class {
        negated: bool,
        atoms: Vec<ClassAtom>,
    },
}

#[derive(Clone)]
enum ClassAtom {
    Char(char),
    Range(char, char),
    Named(&'static str),
}

impl ShellPatternToken {
    fn matches(&self, value: char) -> bool {
        match self {
            Self::Literal(expected) => value == *expected,
            Self::Any => true,
            Self::Star => true,
            Self::Class { negated, atoms } => {
                let matched = atoms.iter().any(|atom| match atom {
                    ClassAtom::Char(expected) => value == *expected,
                    ClassAtom::Range(start, end) => *start <= value && value <= *end,
                    ClassAtom::Named(name) => match *name {
                        "alnum" => value.is_ascii_alphanumeric(),
                        "alpha" => value.is_ascii_alphabetic(),
                        "blank" => matches!(value, ' ' | '\t'),
                        "cntrl" => value.is_ascii_control(),
                        "digit" => value.is_ascii_digit(),
                        "graph" => value.is_ascii_graphic(),
                        "lower" => value.is_ascii_lowercase(),
                        "print" => value.is_ascii_graphic() || value == ' ',
                        "punct" => value.is_ascii_punctuation(),
                        "space" => crate::runtime::char_class::is_posix_space(value),
                        "upper" => value.is_ascii_uppercase(),
                        "xdigit" => value.is_ascii_hexdigit(),
                        _ => false,
                    },
                });
                matched != *negated
            }
        }
    }
}

fn parse_shell_pattern(pattern: &str) -> Vec<ShellPatternToken> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            '*' => {
                if !matches!(tokens.last(), Some(ShellPatternToken::Star)) {
                    tokens.push(ShellPatternToken::Star);
                }
                index += 1;
            }
            '?' => {
                tokens.push(ShellPatternToken::Any);
                index += 1;
            }
            '\\' if index + 1 < chars.len() => {
                tokens.push(ShellPatternToken::Literal(chars[index + 1]));
                index += 2;
            }
            '[' => {
                if let Some((token, end)) = parse_bracket_expression(&chars, index) {
                    tokens.push(token);
                    index = end;
                } else {
                    tokens.push(ShellPatternToken::Literal('['));
                    index += 1;
                }
            }
            literal => {
                tokens.push(ShellPatternToken::Literal(literal));
                index += 1;
            }
        }
    }
    tokens
}

fn parse_bracket_expression(chars: &[char], start: usize) -> Option<(ShellPatternToken, usize)> {
    let mut index = start + 1;
    let negated = matches!(chars.get(index), Some('!' | '^'));
    if negated {
        index += 1;
    }
    let content_start = index;
    let mut content_end = None;
    while index < chars.len() {
        if chars[index] == ']' && index > content_start {
            content_end = Some(index);
            break;
        }
        if chars[index] == '[' && chars.get(index + 1) == Some(&':') {
            let mut class_end = index + 2;
            while class_end + 1 < chars.len()
                && !(chars[class_end] == ':' && chars[class_end + 1] == ']')
            {
                class_end += 1;
            }
            if class_end + 1 < chars.len() {
                index = class_end + 2;
                continue;
            }
        }
        if chars[index] == '\\' && index + 1 < chars.len() {
            index += 2;
        } else {
            index += 1;
        }
    }
    let end = content_end?;
    let mut atoms = Vec::new();
    let mut cursor = content_start;
    while cursor < end {
        if chars[cursor] == '[' && chars.get(cursor + 1) == Some(&':') {
            let mut class_end = cursor + 2;
            while class_end + 1 < end && !(chars[class_end] == ':' && chars[class_end + 1] == ']') {
                class_end += 1;
            }
            if class_end + 1 < end {
                let name: String = chars[cursor + 2..class_end].iter().collect();
                let class_name = match name.as_str() {
                    "alnum" => Some("alnum"),
                    "alpha" => Some("alpha"),
                    "blank" => Some("blank"),
                    "cntrl" => Some("cntrl"),
                    "digit" => Some("digit"),
                    "graph" => Some("graph"),
                    "lower" => Some("lower"),
                    "print" => Some("print"),
                    "punct" => Some("punct"),
                    "space" => Some("space"),
                    "upper" => Some("upper"),
                    "xdigit" => Some("xdigit"),
                    _ => None,
                };
                if let Some(class_name) = class_name {
                    atoms.push(ClassAtom::Named(class_name));
                }
                cursor = class_end + 2;
                continue;
            }
        }

        let (first, next) = if chars[cursor] == '\\' && cursor + 1 < end {
            (chars[cursor + 1], cursor + 2)
        } else {
            (chars[cursor], cursor + 1)
        };
        if next + 1 < end && chars[next] == '-' {
            let (last, after_last) = if chars[next + 1] == '\\' && next + 2 < end {
                (chars[next + 2], next + 3)
            } else {
                (chars[next + 1], next + 2)
            };
            atoms.push(ClassAtom::Range(first, last));
            cursor = after_last;
        } else {
            atoms.push(ClassAtom::Char(first));
            cursor = next;
        }
    }
    if atoms.is_empty() {
        return None;
    }
    Some((ShellPatternToken::Class { negated, atoms }, end + 1))
}
