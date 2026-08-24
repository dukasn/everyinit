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
use crate::planner::ir::{
    CommandTarget, ExecutionPlan, ExecutionStage, PlanNode, PlanWord, PlanWordPart,
};
use crate::runtime::context::{ExecutionContext, TrapSignal};
use crate::runtime::streams::StreamTarget;
use crate::runtime::traits::{BoxFuture, Executable, ScopeManager};
use evin_process::{
    EnvironmentPolicy, OutputPolicy, ProcessHandle, ProcessMode, ProcessSpec, ProcessTermination,
    StdinPolicy,
};
use std::fs;
use tokio::task::JoinSet;

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
                    if !ctx.is_in_trap()
                        && let Some(int_trap) = ctx.get_trap(TrapSignal::Interrupt)
                    {
                        ctx.set_in_trap(true);
                        ctx.push_scope();
                        let _ = int_trap.execute(ctx).await;
                        let _ = ctx.pop_scope();
                        ctx.set_in_trap(false);
                    }
                    ctx.leave_exec();
                    return Ok(130);
                }

                last_status = stage.execute(ctx).await?;
                ctx.set_last_exit(last_status);

                if last_status != 0 {
                    if !ctx.is_in_trap()
                        && let Some(err_trap) = ctx.get_trap(TrapSignal::Err)
                    {
                        ctx.set_in_trap(true);
                        ctx.push_scope();
                        let _ = err_trap.execute(ctx).await;
                        let _ = ctx.pop_scope();
                        ctx.set_in_trap(false);
                    }

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

            if ctx.is_root_exec()
                && !ctx.is_in_trap()
                && let Some(exit_trap) = ctx.get_trap(TrapSignal::Exit)
            {
                ctx.set_in_trap(true);
                ctx.push_scope();
                let _ = exit_trap.execute(ctx).await;
                let _ = ctx.pop_scope();
                ctx.set_in_trap(false);
            }

            Ok(last_status)
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
                if !ctx.is_in_trap()
                    && let Some(int_trap) = ctx.get_trap(TrapSignal::Interrupt)
                {
                    ctx.set_in_trap(true);
                    ctx.push_scope();
                    let _ = int_trap.execute(ctx).await;
                    let _ = ctx.pop_scope();
                    ctx.set_in_trap(false);
                }
                return Ok(130);
            }

            let token = ctx.token().clone();

            tokio::select! {
                biased;
                res = self.execute_inner(ctx) => res,
                _ = token.cancelled() => {
                    ctx.record_failure("interrupted", 0, 130, Some("Execution cancelled".to_string()));
                    if !ctx.is_in_trap()
                        && let Some(int_trap) = ctx.get_trap(TrapSignal::Interrupt) {
                            ctx.set_in_trap(true);
                            ctx.push_scope();
                            let _ = int_trap.execute(ctx).await;
                            let _ = ctx.pop_scope();
                            ctx.set_in_trap(false);
                        }
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

            ExecutionStage::Parallel(branches) => {
                let mut join_set = JoinSet::new();

                for branch in branches {
                    let mut branch_ctx = ctx.clone_isolated();
                    let branch_stage = branch.clone();

                    join_set.spawn(async move {
                        let res = branch_stage.execute(&mut branch_ctx).await;
                        (res, branch_ctx)
                    });
                }

                let mut first_error_code = 0;
                while let Some(join_res) = join_set.join_next().await {
                    let (res, child_ctx) =
                        join_res.map_err(|e| ExecError::JoinError(e.to_string()))?;

                    // Merge child outputs
                    if let Some(out) = child_ctx.get_captured_stdout() {
                        ctx.write_stdout(out.as_bytes());
                    }
                    if let Some(err) = child_ctx.get_captured_stderr() {
                        ctx.write_stderr(err.as_bytes());
                    }

                    let exit_code = res?;
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
                let left_code = left.execute(ctx).await?;
                ctx.set_errexit_shielded(prev_shield);
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
                ctx.push_scope();
                let prev_fail_fast = ctx.fail_fast_enabled();
                ctx.set_fail_fast(true);

                let try_res = try_stage.execute(ctx).await;
                ctx.set_fail_fast(prev_fail_fast);
                let _ = ctx.pop_scope();

                match try_res {
                    Ok(0) => {
                        ctx.set_last_exit(0);
                        Ok(0)
                    }
                    Ok(code) => {
                        if ctx.get_var("ESH_FAILED_CMD").is_none() {
                            ctx.record_failure("block", 0, code, None);
                        } else {
                            ctx.set_last_exit(code);
                        }
                        if let Some(catch_stage) = fallback {
                            ctx.push_scope();
                            let catch_res = catch_stage.execute(ctx).await;
                            let _ = ctx.pop_scope();
                            catch_res
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
                            ctx.push_scope();
                            let catch_res = catch_stage.execute(ctx).await;
                            let _ = ctx.pop_scope();
                            catch_res
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
                for stage in stages {
                    last_code = stage.execute(&mut child_ctx).await?;
                    child_ctx.set_last_exit(last_code);
                    if last_code != 0 && child_ctx.fail_fast_enabled() {
                        break;
                    }
                }
                let _ = child_ctx.pop_scope();
                ctx.set_last_exit(last_code);
                Ok(last_code)
            }

            ExecutionStage::Negation(inner) => {
                let code = inner.execute(ctx).await?;
                let inverted = if code == 0 { 1 } else { 0 };
                ctx.set_last_exit(inverted);
                Ok(inverted)
            }

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
                let cond_status = condition.execute(ctx).await?;
                ctx.set_errexit_shielded(prev_shield);

                if cond_status == 0 {
                    let res = then_branch.execute(ctx).await?;
                    ctx.set_last_exit(res);
                    return Ok(res);
                }

                for (elif_cond, elif_body) in elif_branches {
                    let prev_s = ctx.is_errexit_shielded();
                    ctx.set_errexit_shielded(true);
                    let elif_status = elif_cond.execute(ctx).await?;
                    ctx.set_errexit_shielded(prev_s);

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

                let items: Vec<String> = if let Some(w_list) = words {
                    let mut expanded = Vec::new();
                    for w in w_list {
                        let val = expand_word(w, ctx).await?;
                        expanded.push(val);
                    }
                    expanded
                } else {
                    ctx.positional_args().to_vec()
                };

                for item in items {
                    ctx.set_var(var, &item);

                    last_status = body.execute(ctx).await?;
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
                    let cond_status = condition.execute(ctx).await?;
                    ctx.set_errexit_shielded(prev_shield);

                    let should_run = if *is_until {
                        cond_status != 0
                    } else {
                        cond_status == 0
                    };

                    if !should_run {
                        break;
                    }

                    last_status = body.execute(ctx).await?;
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
                let mut last_status = 0;
                let mut fallthrough = false;

                for arm in arms {
                    let mut matched = fallthrough;

                    if !matched {
                        for pat in &arm.patterns {
                            let pat_str = expand_word(pat, ctx).await?;
                            if glob_match_case(&pat_str, &target_val) {
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

                        if ctx.is_exiting() || ctx.is_returning() || ctx.is_cancelled() {
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

        let status = execute_plan_node(node, &mut step_ctx).await?;
        statuses.push(status);

        if !is_last {
            current_input = step_ctx.get_captured_stdout().map(|s| s.into_bytes());
        } else {
            if let Some(out) = step_ctx.get_captured_stdout() {
                ctx.write_stdout(out.as_bytes());
            }
            if let Some(err) = step_ctx.get_captured_stderr() {
                ctx.write_stderr(err.as_bytes());
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

async fn execute_plan_node(node: &PlanNode, ctx: &mut ExecutionContext) -> Result<i32, ExecError> {
    // 1. Process Input Redirections / Here-Docs / Here-Strings
    for redir in &node.redirections {
        if redir.op == RedirectionOp::RedirectIn {
            let target_path = expand_word(&redir.target, ctx).await?;
            let target = StreamTarget::from_redirection(redir.op, &target_path, ctx.cwd())?;
            match target {
                StreamTarget::Null => ctx.set_stdin(Vec::new()),
                StreamTarget::File { path, .. } => {
                    let content = fs::read(&path).map_err(|e| {
                        ExecError::NotFound(format!(
                            "Cannot open input file '{}': {}",
                            path.display(),
                            e
                        ))
                    })?;
                    ctx.set_stdin(content);
                }
                StreamTarget::Inherit | StreamTarget::Stderr | StreamTarget::Stdout => {}
            }
        } else if redir.op == RedirectionOp::RedirectHereString
            || redir.op == RedirectionOp::RedirectHereDoc
        {
            let content = expand_word(&redir.target, ctx).await?;
            ctx.set_stdin(format!("{}\n", content).into_bytes());
        }
    }

    // 2. Apply command-scoped assignments
    for asgn in &node.assignments {
        if let Some(array_vals) = &asgn.array_values {
            let mut items = Vec::new();
            for v in array_vals {
                items.push(expand_word(v, ctx).await?);
            }
            ctx.set_scoped_val(
                &asgn.name,
                crate::runtime::context::ShellValue::Array(items),
            );
        } else if let Some(idx_word) = &asgn.index {
            let sub = expand_word(idx_word, ctx).await?;
            let val = if let Some(v) = &asgn.value {
                expand_word(v, ctx).await?
            } else {
                String::new()
            };
            if let Ok(num_idx) = sub.parse::<usize>() {
                ctx.set_array_elem(&asgn.name, num_idx, val);
            } else {
                ctx.set_map_elem(&asgn.name, sub, val);
            }
        } else if let Some(val_word) = &asgn.value {
            let val = expand_word(val_word, ctx).await?;
            ctx.set_scoped_var(&asgn.name, val);
        }
    }

    if node.raw_name == "=" {
        return Ok(0);
    }

    // 3. Expand arguments
    let mut expanded_args = Vec::new();
    for arg in &node.args {
        let val = expand_word(arg, ctx).await?;
        expanded_args.push(val);
    }

    if ctx.options().xtrace {
        let trace = if expanded_args.is_empty() {
            format!("+ {}\n", node.raw_name)
        } else {
            format!("+ {} {}\n", node.raw_name, expanded_args.join(" "))
        };
        ctx.write_stderr(trace.as_bytes());
    }

    // Check if output redirections are present
    let has_output_redirs = node.redirections.iter().any(|r| {
        r.op != RedirectionOp::RedirectIn
            && r.op != RedirectionOp::RedirectHereString
            && r.op != RedirectionOp::RedirectHereDoc
    });

    let (result, captured_out, captured_err) = if has_output_redirs {
        let mut call_ctx = ctx.clone_isolated();
        call_ctx.enable_capture();
        let res =
            dispatch_node_target(&node.target, &node.raw_name, &expanded_args, &mut call_ctx).await;
        let out = call_ctx.get_captured_stdout().unwrap_or_default();
        let err = call_ctx.get_captured_stderr().unwrap_or_default();
        (res, Some(out), Some(err))
    } else {
        let res = dispatch_node_target(&node.target, &node.raw_name, &expanded_args, ctx).await;
        (res, None, None)
    };

    // 4. Apply output redirections
    if has_output_redirs {
        let out_bytes = captured_out.unwrap_or_default().into_bytes();
        let err_bytes = captured_err.unwrap_or_default().into_bytes();

        let mut stdout_redirected = false;
        let mut stderr_redirected = false;

        for redir in &node.redirections {
            let target_path = expand_word(&redir.target, ctx).await?;
            let target = StreamTarget::from_redirection(redir.op, &target_path, ctx.cwd())?;

            match redir.op {
                RedirectionOp::RedirectOut | RedirectionOp::RedirectAppend => {
                    target.write_data(&out_bytes)?;
                    stdout_redirected = true;
                }
                RedirectionOp::RedirectStderr | RedirectionOp::RedirectStderrAppend => {
                    target.write_data(&err_bytes)?;
                    stderr_redirected = true;
                }
                RedirectionOp::RedirectAll | RedirectionOp::RedirectAllAppend => {
                    let mut combined = out_bytes.clone();
                    combined.extend_from_slice(&err_bytes);
                    target.write_data(&combined)?;
                    stdout_redirected = true;
                    stderr_redirected = true;
                }
                _ => {}
            }
        }

        if !stdout_redirected && !out_bytes.is_empty() {
            ctx.write_stdout(&out_bytes);
        }
        if !stderr_redirected && !err_bytes.is_empty() {
            ctx.write_stderr(&err_bytes);
        }
    }

    match result {
        Ok(code) => {
            ctx.set_last_exit(code);
            if code != 0 {
                ctx.record_failure(&node.raw_name, node.span.line, code, None);
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
            Ok(code)
        }
        Err(e) => {
            let code = e.exit_code();
            ctx.record_failure(&node.raw_name, node.span.line, code, Some(e.to_string()));
            if !ctx.is_in_trap()
                && let Some(err_trap) = ctx.get_trap(TrapSignal::Err)
            {
                ctx.set_in_trap(true);
                ctx.push_scope();
                let _ = err_trap.execute(ctx).await;
                let _ = ctx.pop_scope();
                ctx.set_in_trap(false);
            }
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
        let mut func_args = vec![raw_name.to_string()];
        func_args.extend_from_slice(args);
        ctx.set_positional_args(func_args);

        let status = func_plan.execute(ctx).await;
        let return_code = ctx.take_return();
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
            let mut func_args = vec![raw_name.to_string()];
            func_args.extend_from_slice(args);
            ctx.set_positional_args(func_args);

            let status = plan.execute(ctx).await;
            let return_code = ctx.take_return();
            let _ = ctx.pop_scope();
            match return_code {
                Some(rc) => Ok(rc),
                None => status,
            }
        }

        CommandTarget::External(bin_path) => {
            let input_data = ctx.take_stdin();
            let stdin_is_piped = input_data.is_some();
            let capture = ctx.is_capturing();
            let mode = if capture || stdin_is_piped {
                ProcessMode::SupervisedAutomation
            } else {
                ProcessMode::InteractiveForeground
            };
            let mut spec = ProcessSpec::new(bin_path, ctx.cwd(), mode)
                .args(args.iter().map(String::as_str))
                .environment(EnvironmentPolicy::InheritAndOverride, ctx.env_vars());
            spec.stdin = match input_data {
                Some(bytes) => StdinPolicy::Bytes(bytes),
                None if capture => StdinPolicy::Null,
                None => StdinPolicy::Inherit,
            };
            spec.stdout = if capture {
                OutputPolicy::Capture
            } else {
                OutputPolicy::Inherit
            };
            spec.stderr = if capture {
                OutputPolicy::Capture
            } else {
                OutputPolicy::Inherit
            };

            let result = ProcessHandle::spawn(spec, None)
                .map_err(|error| ExecError::IoError(error.to_string()))?
                .wait(ctx.token().clone())
                .await
                .map_err(|error| ExecError::IoError(error.to_string()))?;

            if let Some(stdout) = result.stdout {
                ctx.write_stdout(&stdout);
            }
            if let Some(stderr) = result.stderr {
                ctx.write_stderr(&stderr);
            }

            Ok(match result.termination {
                ProcessTermination::Exited(status) => status.code().unwrap_or(1),
                ProcessTermination::Cancelled => 130,
            })
        }
    }
}

pub async fn expand_word(word: &PlanWord, ctx: &mut ExecutionContext) -> Result<String, ExecError> {
    let mut result = String::new();

    for part in &word.parts {
        match part {
            PlanWordPart::Literal(s) => {
                result.push_str(s);
            }
            PlanWordPart::SingleQuoted(s) => {
                result.push_str(s);
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
                    let plan = compile_script(sub_script).map_err(|e| {
                        ExecError::Custom(format!("Command substitution parse error: {}", e))
                    })?;
                    let mut sub_ctx = ctx.clone_isolated();
                    sub_ctx.enable_capture();

                    plan.execute(&mut sub_ctx).await?;

                    let stdout = sub_ctx.get_captured_stdout().unwrap_or_default();
                    let trimmed = stdout.trim_end_matches(&['\r', '\n'][..]);
                    result.push_str(trimmed);
                }
            }
        }
    }

    Ok(result)
}

pub async fn expand_parameter_expr(
    expr: &str,
    ctx: &mut ExecutionContext,
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

    // 1. ${#VAR} / ${#VAR[@]} / ${#VAR[idx]} -> string length, element count, or positional parameter count
    if let Some(var_name) = expr.strip_prefix('#') {
        if let Some((name, sub)) = var_name.split_once('[') {
            let sub_clean = sub.trim_end_matches(']');
            if sub_clean == "@" || sub_clean == "*" {
                if let Some(val) = ctx.get_shell_val(name) {
                    return Ok(val.len().to_string());
                }
                return Ok("0".to_string());
            } else {
                if let Some(val) = ctx.get_shell_val(name)
                    && let Some(item) = val.get_subscript(sub_clean)
                {
                    return Ok(item.len().to_string());
                }
                return Ok("0".to_string());
            }
        }
        if var_name == "@" || var_name == "*" || var_name == "#" {
            return Ok(ctx.positional_args().len().to_string());
        }
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(val.len().to_string());
    }

    // 1.5 Subscript Access: ${VAR[sub]} or ${VAR[sub]:-default} or ${VAR[sub]##pattern}
    if let Some((var_name, sub_part)) = expr.split_once('[')
        && is_valid_var_ident(var_name)
        && let Some((sub_spec, modifier)) = sub_part.split_once(']')
    {
        let resolved_sub = if sub_spec.contains('$') {
            Box::pin(expand_double_quoted(sub_spec, ctx)).await?
        } else {
            sub_spec.to_string()
        };
        let sub_clean = resolved_sub.as_str();
        let val_str = if let Some(val) = ctx.get_shell_val(var_name) {
            val.get_subscript(sub_clean).unwrap_or_default()
        } else {
            String::new()
        };

        if modifier.is_empty() {
            return Ok(val_str);
        }

        if let Some((_, default_val)) = modifier.split_once(":-") {
            if !val_str.is_empty() {
                return Ok(val_str);
            }
            return Box::pin(expand_double_quoted(default_val, ctx)).await;
        }
        if let Some(pattern) = modifier.strip_prefix("##") {
            return Ok(strip_prefix_pattern(&val_str, pattern, true));
        }
        if let Some(pattern) = modifier.strip_prefix('#') {
            return Ok(strip_prefix_pattern(&val_str, pattern, false));
        }
        if let Some(pattern) = modifier.strip_prefix("%%") {
            return Ok(strip_suffix_pattern(&val_str, pattern, true));
        }
        if let Some(pattern) = modifier.strip_prefix('%') {
            return Ok(strip_suffix_pattern(&val_str, pattern, false));
        }
        if let Some(slice_spec) = modifier.strip_prefix(':') {
            let parts: Vec<&str> = slice_spec.split(':').collect();
            if let Ok(offset) = parts[0].parse::<usize>() {
                let chars: Vec<char> = val_str.chars().collect();
                if offset < chars.len() {
                    if parts.len() > 1
                        && let Ok(len) = parts[1].parse::<usize>()
                    {
                        let end = (offset + len).min(chars.len());
                        return Ok(chars[offset..end].iter().collect());
                    }
                    return Ok(chars[offset..].iter().collect());
                } else {
                    return Ok(String::new());
                }
            }
        }
        return Ok(val_str);
    }

    // 2. ${VAR:-default}
    if let Some((name, default_val)) = expr.split_once(":-") {
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        return Box::pin(expand_double_quoted(default_val, ctx)).await;
    }

    // 3. ${VAR:=default}
    if let Some((name, default_val)) = expr.split_once(":=") {
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        let evaluated_default = Box::pin(expand_double_quoted(default_val, ctx)).await?;
        ctx.set_scoped_var(name, evaluated_default.clone());
        return Ok(evaluated_default);
    }

    // 4. ${VAR:?error_msg}
    if let Some((name, err_msg)) = expr.split_once(":?") {
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Ok(v);
        }
        let evaluated_msg = if err_msg.is_empty() {
            format!("parameter '{}' is null or unset", name)
        } else {
            Box::pin(expand_double_quoted(err_msg, ctx)).await?
        };
        ctx.write_stderr(format!("esh: {}\n", evaluated_msg).as_bytes());
        return Err(ExecError::Custom(evaluated_msg));
    }

    // 5. ${VAR:+alternate}
    if let Some((name, alt_val)) = expr.split_once(":+") {
        let val = ctx.get_var(name);
        if let Some(v) = val
            && !v.is_empty()
        {
            return Box::pin(expand_double_quoted(alt_val, ctx)).await;
        }
        return Ok(String::new());
    }

    // 6. Substring Slicing: ${VAR:offset:length} or ${VAR:offset}
    if let Some((var_name, slice_spec)) = expr.split_once(':')
        && is_valid_var_ident(var_name)
    {
        let val = ctx.get_var(var_name).unwrap_or_default();
        let parts: Vec<&str> = slice_spec.split(':').collect();
        if let Ok(offset) = parts[0].parse::<usize>() {
            let chars: Vec<char> = val.chars().collect();
            if offset < chars.len() {
                if parts.len() > 1
                    && let Ok(len) = parts[1].parse::<usize>()
                {
                    let end = (offset + len).min(chars.len());
                    return Ok(chars[offset..end].iter().collect());
                }
                return Ok(chars[offset..].iter().collect());
            } else {
                return Ok(String::new());
            }
        }
    }

    // 7. Search and replace: ${VAR//pat/rep} or ${VAR/pat/rep}
    if let Some((var_name, rest)) = expr.split_once("//")
        && is_valid_var_ident(var_name)
    {
        let (pattern, replacement) = rest.split_once('/').unwrap_or((rest, ""));
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(val.replace(pattern, replacement));
    }
    if let Some((var_name, rest)) = expr.split_once('/')
        && is_valid_var_ident(var_name)
    {
        let (pattern, replacement) = rest.split_once('/').unwrap_or((rest, ""));
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(val.replacen(pattern, replacement, 1));
    }

    // 8. Longest/Shortest Prefix Stripping: ${VAR##pat} and ${VAR#pat}
    if let Some((var_name, pattern)) = expr.split_once("##")
        && is_valid_var_ident(var_name)
    {
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(strip_prefix_pattern(&val, pattern, true));
    }
    if let Some((var_name, pattern)) = expr.split_once('#')
        && is_valid_var_ident(var_name)
    {
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(strip_prefix_pattern(&val, pattern, false));
    }

    // 9. Longest/Shortest Suffix Stripping: ${VAR%%pat} and ${VAR%pat}
    if let Some((var_name, pattern)) = expr.split_once("%%")
        && is_valid_var_ident(var_name)
    {
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(strip_suffix_pattern(&val, pattern, true));
    }
    if let Some((var_name, pattern)) = expr.split_once('%')
        && is_valid_var_ident(var_name)
    {
        let val = ctx.get_var(var_name).unwrap_or_default();
        return Ok(strip_suffix_pattern(&val, pattern, false));
    }

    // 10. Simple variable lookup
    if let Some(val) = ctx.get_var(expr) {
        Ok(val)
    } else if ctx.options().nounset
        && !matches!(
            expr,
            "?" | "@" | "*" | "#" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"
        )
    {
        Err(ExecError::Custom(format!("{}: unbound variable", expr)))
    } else {
        Ok(String::new())
    }
}

fn is_valid_var_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn strip_prefix_pattern(text: &str, pattern: &str, greedy: bool) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    if pattern == "*" {
        return String::new();
    }
    if let Some(suffix_pat) = pattern.strip_prefix('*') {
        if greedy {
            if let Some(idx) = text.rfind(suffix_pat) {
                return text[idx + suffix_pat.len()..].to_string();
            }
        } else if let Some(idx) = text.find(suffix_pat) {
            return text[idx + suffix_pat.len()..].to_string();
        }
    } else if let Some(stripped) = text.strip_prefix(pattern) {
        return stripped.to_string();
    }
    text.to_string()
}

fn strip_suffix_pattern(text: &str, pattern: &str, greedy: bool) -> String {
    if pattern.is_empty() {
        return text.to_string();
    }
    if pattern == "*" {
        return String::new();
    }
    if let Some(prefix_pat) = pattern.strip_suffix('*') {
        if greedy {
            if let Some(idx) = text.find(prefix_pat) {
                return text[..idx].to_string();
            }
        } else if let Some(idx) = text.rfind(prefix_pat) {
            return text[..idx].to_string();
        }
    } else if let Some(stripped) = text.strip_suffix(pattern) {
        return stripped.to_string();
    }
    text.to_string()
}

pub async fn expand_double_quoted(
    content: &str,
    ctx: &mut ExecutionContext,
) -> Result<String, ExecError> {
    let mut res = String::new();
    let bytes = content.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'\\' && i + 1 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'$' || next == b'"' || next == b'\\' || next == b'`' {
                res.push(next as char);
                i += 2;
                continue;
            }
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

                plan.execute(&mut sub_ctx).await?;

                let stdout = sub_ctx.get_captured_stdout().unwrap_or_default();
                let trimmed = stdout.trim_end_matches(&['\r', '\n'][..]);
                res.push_str(trimmed);
                continue;
            }

            // ${VAR} or ${VAR:-default}
            if bytes[i] == b'{' {
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] != b'}' {
                    i += 1;
                }
                let expr = &content[start..i];
                if i < bytes.len() && bytes[i] == b'}' {
                    i += 1;
                }
                let expanded = Box::pin(expand_parameter_expr(expr, ctx)).await?;
                res.push_str(&expanded);
                continue;
            }

            // Special single-character variables: $?, $@, $*, $#, $0..$9
            if bytes[i] == b'?'
                || bytes[i] == b'@'
                || bytes[i] == b'*'
                || bytes[i] == b'#'
                || bytes[i].is_ascii_digit()
            {
                let name = &content[i..i + 1];
                i += 1;
                if let Some(val) = ctx.get_var(name) {
                    res.push_str(&val);
                }
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
                if let Some(val) = ctx.get_var(name) {
                    res.push_str(&val);
                } else if ctx.options().nounset
                    && !matches!(
                        name,
                        "?" | "@"
                            | "*"
                            | "#"
                            | "0"
                            | "1"
                            | "2"
                            | "3"
                            | "4"
                            | "5"
                            | "6"
                            | "7"
                            | "8"
                            | "9"
                    )
                {
                    return Err(ExecError::Custom(format!("{}: unbound variable", name)));
                }
            }
            continue;
        }

        res.push(b as char);
        i += 1;
    }

    Ok(res)
}

fn glob_match_case(pattern: &str, text: &str) -> bool {
    glob_match_slice(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_slice(pat: &[u8], text: &[u8]) -> bool {
    let mut p = 0;
    let mut t = 0;
    let mut star_p = None;
    let mut match_t = 0;

    while t < text.len() {
        if p < pat.len() && pat[p] == b'?' {
            p += 1;
            t += 1;
        } else if p < pat.len() && pat[p] == b'[' {
            let mut end = p + 1;
            let mut negated = false;
            if end < pat.len() && (pat[end] == b'!' || pat[end] == b'^') {
                negated = true;
                end += 1;
            }
            let mut matched_class = false;
            let target_b = text[t];

            while end < pat.len() && pat[end] != b']' {
                if end + 2 < pat.len() && pat[end + 1] == b'-' && pat[end + 2] != b']' {
                    let start_b = pat[end];
                    let stop_b = pat[end + 2];
                    if target_b >= start_b && target_b <= stop_b {
                        matched_class = true;
                    }
                    end += 3;
                } else {
                    if pat[end] == target_b {
                        matched_class = true;
                    }
                    end += 1;
                }
            }

            if (matched_class && !negated) || (!matched_class && negated) {
                p = end + 1;
                t += 1;
            } else if let Some(sp) = star_p {
                p = sp + 1;
                match_t += 1;
                t = match_t;
            } else {
                return false;
            }
        } else if p < pat.len() && pat[p] == b'*' {
            star_p = Some(p);
            p += 1;
            match_t = t;
        } else if p < pat.len() && pat[p] == text[t] {
            p += 1;
            t += 1;
        } else if let Some(sp) = star_p {
            p = sp + 1;
            match_t += 1;
            t = match_t;
        } else {
            return false;
        }
    }

    while p < pat.len() && pat[p] == b'*' {
        p += 1;
    }

    p == pat.len()
}
