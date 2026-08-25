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

//! Build-time execution for terminal tapes.
//!
//! This deliberately lives in the docs action instead of `evin-doc`: the
//! document crate remains usable by parsers, language servers, and static
//! renderers without depending on a host shell or a PTY implementation.

use crate::v1::ast::{DocNode, DocSiteAst};
use esh::syntax::{TokenKind, scan_tokens};
use esh::{Engine, ExecutionContext};
use evin_doc::tape::{
    TapeCommand, TapeConfig, TapeRuntime, TapeScript, TerminalColor, TerminalFrame, VirtualScreen,
    render_svg,
};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TRANSCRIPT_LIMIT: usize = 4096;

#[derive(Debug, Clone)]
struct PathPresentation {
    workspace_paths: Vec<String>,
    path_root: String,
    max_workspace_path_len: usize,
}

impl PathPresentation {
    fn new(workspace_root: &Path, path_root: &str) -> Result<Self, String> {
        let workspace = workspace_root
            .canonicalize()
            .map_err(|error| format!("could not canonicalize tape workspace: {error}"))?
            .to_string_lossy()
            .into_owned();
        let mut workspace_paths = vec![workspace.clone()];
        let slash_normalized = workspace.replace('\\', "/");
        if slash_normalized != workspace {
            workspace_paths.push(slash_normalized);
        }
        workspace_paths.sort_by_key(|path| std::cmp::Reverse(path.len()));
        let max_workspace_path_len = workspace_paths.iter().map(String::len).max().unwrap_or(0);
        Ok(Self {
            workspace_paths,
            path_root: path_root.to_string(),
            max_workspace_path_len,
        })
    }

    fn present(&self, text: &str) -> String {
        let mut output = String::with_capacity(text.len());
        let mut cursor = 0;
        while cursor < text.len() {
            let remaining = &text[cursor..];
            if let Some(path) = self.workspace_paths.iter().find(|path| {
                remaining.starts_with(path.as_str()) && path_boundary(&remaining[path.len()..])
            }) {
                output.push_str(&self.path_root);
                cursor += path.len();
                continue;
            }
            let character = remaining.chars().next().expect("cursor is in bounds");
            output.push(character);
            cursor += character.len_utf8();
        }
        output
    }
}

fn path_boundary(remaining: &str) -> bool {
    remaining.chars().next().is_none_or(|character| {
        matches!(character, '/' | '\\')
            || !matches!(character, 'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '-' | '.')
    })
}

/// Holds enough trailing text to recognize a workspace path split across PTY
/// reads, then presents complete chunks without touching ANSI/VT bytes.
#[derive(Debug, Clone)]
struct IncrementalPathPresentation {
    presentation: PathPresentation,
    pending: String,
}

impl IncrementalPathPresentation {
    fn new(presentation: PathPresentation) -> Self {
        Self {
            presentation,
            pending: String::new(),
        }
    }

    fn push(&mut self, text: &str) -> String {
        self.pending.push_str(text);
        let safe_limit = self
            .pending
            .len()
            .saturating_sub(self.presentation.max_workspace_path_len.saturating_add(1));
        if safe_limit == 0 {
            return String::new();
        }
        let mut output = String::new();
        let mut cursor = 0;
        while cursor < safe_limit {
            let remaining = &self.pending[cursor..];
            if let Some(path) = self.presentation.workspace_paths.iter().find(|path| {
                remaining.starts_with(path.as_str()) && path_boundary(&remaining[path.len()..])
            }) {
                output.push_str(&self.presentation.path_root);
                cursor += path.len();
                continue;
            }
            let character = remaining.chars().next().expect("cursor is in bounds");
            output.push(character);
            cursor += character.len_utf8();
        }
        self.pending.drain(..cursor);
        output
    }

    fn finish(&mut self) -> String {
        self.presentation
            .present(&std::mem::take(&mut self.pending))
    }
}

#[derive(Debug, Clone, Copy)]
enum Shell {
    #[cfg(unix)]
    Bash,
    #[cfg(unix)]
    Zsh,
    #[cfg(windows)]
    PowerShell,
    #[cfg(windows)]
    Cmd,
}

impl Shell {
    fn command(self) -> CommandBuilder {
        match self {
            #[cfg(unix)]
            Self::Bash => {
                let mut command = CommandBuilder::new("bash");
                command.args(["--noprofile", "--norc", "-i"]);
                command
            }
            #[cfg(unix)]
            Self::Zsh => {
                let mut command = CommandBuilder::new("zsh");
                command.args(["-d", "-f", "-i"]);
                command
            }
            #[cfg(windows)]
            Self::PowerShell => {
                let mut command = CommandBuilder::new("powershell.exe");
                command.args(["-NoLogo", "-NoProfile", "-NoExit"]);
                command
            }
            #[cfg(windows)]
            Self::Cmd => CommandBuilder::new("cmd.exe"),
        }
    }

    fn setup(self, marker: &str) -> String {
        match self {
            #[cfg(unix)]
            Self::Bash => format!(
                "PROMPT_COMMAND='__evin_status=$?'; PS1='$ \\[\\036{marker}_${{__evin_status}}__\\037\\]'\n"
            ),
            #[cfg(unix)]
            Self::Zsh => format!(
                "function precmd() {{ EVIN_STATUS=$?; }}; setopt prompt_subst; PS1='$ %{{\\036{marker}_${{EVIN_STATUS}}__\\037%}}'\n"
            ),
            #[cfg(windows)]
            Self::PowerShell => format!(
                "function prompt {{ $status = if ($?) {{ 0 }} else {{ 1 }}; Write-Host -NoNewline '$ '; Write-Host -NoNewline ([char]30 + '{marker}_' + $status + '__' + [char]31); '' }}\r\n"
            ),
            #[cfg(windows)]
            Self::Cmd => format!("prompt $G$E{marker}_%errorlevel%__\r\n"),
        }
    }

    fn prompt(self) -> &'static str {
        "$ "
    }
}

/// Compile every tape in a site. The page source path is included in every
/// error so a build failure leads authors directly back to the tape block.
pub fn compile_site(site: &mut DocSiteAst, workspace_root: &Path) -> Result<(), String> {
    for page in &mut site.pages {
        compile_nodes(&mut page.ast, workspace_root, &page.source_path)?;
    }
    Ok(())
}

fn compile_nodes(nodes: &mut [DocNode], workspace_root: &Path, source: &str) -> Result<(), String> {
    for node in nodes {
        match node {
            DocNode::Tape {
                script,
                config,
                rendered_svg,
            } => {
                let parsed = TapeScript::parse(script)
                    .map_err(|error| format!("{source}: invalid tape script: {error}"))?;
                let svg = match config.runtime {
                    TapeRuntime::Esh => run_esh_tape(config, workspace_root, &parsed),
                    TapeRuntime::System => Recording::start(config, workspace_root)
                        .map_err(|error| format!("could not start system tape shell: {error}"))?
                        .run(&parsed),
                };
                *rendered_svg =
                    Some(svg.map_err(|error| format!("{source}: terminal tape failed: {error}"))?);
            }
            DocNode::Directive { name, args, .. } if name == "tape-error" => {
                return Err(format!(
                    "{source}: invalid tape attributes: {}",
                    args.join(" ")
                ));
            }
            DocNode::Admonition { children, .. }
            | DocNode::Directive { body: children, .. }
            | DocNode::LspSymbol { children, .. } => {
                compile_nodes(children, workspace_root, source)?
            }
            DocNode::List { items, .. } => {
                for item in items {
                    compile_nodes(item, workspace_root, source)?;
                }
            }
            DocNode::TaskList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, workspace_root, source)?;
                }
            }
            DocNode::DefinitionList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, workspace_root, source)?;
                }
            }
            DocNode::OptionList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, workspace_root, source)?;
                }
            }
            DocNode::FieldList { items } => {
                for item in items {
                    compile_nodes(&mut item.body, workspace_root, source)?;
                }
            }
            DocNode::Stepper { steps } => {
                for step in steps {
                    compile_nodes(&mut step.body, workspace_root, source)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Runs the portable executor away from the synchronous documentation renderer.
/// A tape owns this runtime, engine, and context for its entire session.
fn run_esh_tape(
    config: &TapeConfig,
    workspace_root: &Path,
    script: &TapeScript,
) -> Result<String, String> {
    let config = config.clone();
    let workspace_root = workspace_root.to_path_buf();
    let script = script.clone();
    let (sender, receiver) = mpsc::sync_channel(1);
    let worker = thread::spawn(move || {
        let result = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(async move {
                let recording = EshRecording::start(config, workspace_root)?;
                recording.run(&script).await
            }),
            Err(error) => Err(format!("could not create ESH tape runtime: {error}")),
        };
        let _ = sender.send(result);
    });
    let result = receiver.recv();
    worker
        .join()
        .map_err(|_| "ESH tape runtime panicked".to_string())?;
    result.map_err(|_| "ESH tape runtime stopped unexpectedly".to_string())?
}

struct EshRecording {
    config: TapeConfig,
    paths: PathPresentation,
    engine: Engine,
    context: ExecutionContext,
    input: String,
    last_output: String,
    captured_stdout_bytes: usize,
    captured_stderr_bytes: usize,
    transcript: VecDeque<u8>,
    output_bytes: usize,
    pending_status: Option<i32>,
    screen: VirtualScreen,
    frames: Vec<TerminalFrame>,
    frame_overflow: bool,
    clock_ms: u32,
}

impl EshRecording {
    fn start(config: TapeConfig, workspace_root: std::path::PathBuf) -> Result<Self, String> {
        let paths = PathPresentation::new(&workspace_root, &config.path_root)?;
        let mut context = ExecutionContext::new();
        context
            .set_cwd(workspace_root)
            .map_err(|error| format!("could not set ESH tape working directory: {error}"))?;
        // ESH captures child streams through pipes, so programs normally lose
        // their terminal styling. Declare the recording as a color-capable
        // virtual terminal; EVIN also recognizes this marker and preserves
        // its Matter-mappable ANSI diagnostics in captured output.
        context.set_env("EVIN_TAPE", "1");
        context.set_env("TERM", "xterm-256color");
        context.set_env("COLORTERM", "truecolor");
        context.set_env("CLICOLOR_FORCE", "1");
        context.set_env("FORCE_COLOR", "1");
        context.enable_capture();

        let mut recording = Self {
            screen: VirtualScreen::new(config.rows, config.cols),
            config,
            paths,
            engine: Engine::new(),
            context,
            input: String::new(),
            last_output: String::new(),
            captured_stdout_bytes: 0,
            captured_stderr_bytes: 0,
            transcript: VecDeque::new(),
            output_bytes: 0,
            pending_status: None,
            frames: Vec::new(),
            frame_overflow: false,
            clock_ms: 0,
        };
        recording.screen.write_str("$ ");
        recording.capture();
        Ok(recording)
    }

    async fn run(mut self, script: &TapeScript) -> Result<String, String> {
        let mut needs_wait = false;
        for command in &script.commands {
            match command {
                TapeCommand::Type(value) => {
                    self.type_text(value);
                    needs_wait = true;
                }
                TapeCommand::Backspace(count) => {
                    for _ in 0..*count {
                        self.input.pop();
                        self.screen.write_str("\x08");
                        self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
                        self.capture();
                    }
                    needs_wait = true;
                }
                TapeCommand::Ctrl(letter) => {
                    return Err(self.failure_with_tail(&format!(
                        "ESH tapes do not support ctrl {letter}; use runtime=system for terminal control keys"
                    )));
                }
                TapeCommand::Enter => {
                    self.screen.write_str("\r\n");
                    self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
                    self.capture();
                    self.evaluate_input().await?;
                    needs_wait = true;
                }
                TapeCommand::Sleep(duration) => {
                    self.clock_ms = self.clock_ms.saturating_add(*duration);
                    self.capture();
                }
                TapeCommand::Expect(expected) => {
                    if !self.last_output.contains(expected) {
                        return Err(self.failure_with_tail(&format!(
                            "ESH tape expected completed output containing {expected:?}; ESH cannot wait for interactive prompts (use runtime=system)"
                        )));
                    }
                    self.capture();
                }
                TapeCommand::Wait => {
                    self.wait()?;
                    needs_wait = false;
                }
            }
        }
        if needs_wait {
            self.wait()?;
        }
        if self.frame_overflow {
            return Err(self.failure_with_tail("terminal recording exceeded max-frames"));
        }
        Ok(render_svg(
            &self.config,
            &deduplicate(&self.frames),
            "A recorded ESH terminal session. The final frame remains visible when reduced motion is enabled.",
        ))
    }

    fn type_text(&mut self, value: &str) {
        self.input.push_str(value);
        for (character, color) in semantic_characters(value) {
            self.screen.write_styled_char(character, color);
            self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
            self.capture();
        }
    }

    async fn evaluate_input(&mut self) -> Result<(), String> {
        if let Some(status) = self.pending_status
            && status != 0
        {
            return Err(self.failure_with_tail(&format!("command exited with status {status}")));
        }
        let input = std::mem::take(&mut self.input);
        let timeout = Duration::from_millis(self.config.timeout_ms);
        let cancellation = self.context.token().clone();
        let result = tokio::select! {
            result = self.engine.eval_str(&input, &mut self.context) => result
                .map_err(|error| self.failure_with_tail(&format!("ESH evaluation failed: {error}"))),
            _ = tokio::time::sleep(timeout) => {
                cancellation.cancel();
                return Err(self.failure_with_tail("ESH command timed out and was cancelled"));
            }
        }?;
        self.pending_status = Some(result);
        self.flush_captured_output()?;
        self.screen.write_str("$ ");
        Ok(())
    }

    fn flush_captured_output(&mut self) -> Result<(), String> {
        let stdout = self.context.get_captured_stdout().unwrap_or_default();
        let stderr = self.context.get_captured_stderr().unwrap_or_default();
        let Some(new_stdout) = stdout.get(self.captured_stdout_bytes..) else {
            return Err(self.failure_with_tail("invalid ESH captured output stream"));
        };
        let Some(new_stderr) = stderr.get(self.captured_stderr_bytes..) else {
            return Err(self.failure_with_tail("invalid ESH captured output stream"));
        };
        let new_output = format!("{new_stdout}{new_stderr}");
        if !new_output.is_empty() {
            self.output_bytes = self.output_bytes.saturating_add(new_output.len());
            if self.output_bytes > self.config.max_output_bytes {
                return Err(self.failure_with_tail("terminal output exceeded max-output"));
            }
            self.transcript.extend(new_output.bytes());
            while self.transcript.len() > TRANSCRIPT_LIMIT {
                self.transcript.pop_front();
            }
            self.screen.write_str(&self.paths.present(&new_output));
            self.last_output.push_str(&new_output);
        }
        self.captured_stdout_bytes = stdout.len();
        self.captured_stderr_bytes = stderr.len();
        Ok(())
    }

    fn wait(&mut self) -> Result<(), String> {
        if let Some(status) = self.pending_status.take()
            && status != 0
        {
            return Err(self.failure_with_tail(&format!("command exited with status {status}")));
        }
        self.clock_ms = self.clock_ms.saturating_add(self.config.command_settle_ms);
        self.capture();
        Ok(())
    }

    fn capture(&mut self) {
        let frame = self.screen.capture(self.clock_ms);
        if self.frames.last().is_some_and(|previous| {
            previous.cells == frame.cells
                && previous.cursor_row == frame.cursor_row
                && previous.cursor_col == frame.cursor_col
        }) {
            if let Some(previous) = self.frames.last_mut() {
                previous.timestamp_ms = frame.timestamp_ms;
            }
        } else {
            self.frames.push(frame);
        }
        if self.frames.len() > self.config.max_frames {
            self.frame_overflow = true;
            self.frames.truncate(self.config.max_frames);
        }
    }

    fn failure_with_tail(&self, message: &str) -> String {
        let tail_bytes = self.transcript.iter().copied().collect::<Vec<_>>();
        let tail = String::from_utf8_lossy(&tail_bytes);
        if tail.trim().is_empty() {
            message.to_string()
        } else {
            format!(
                "{message}; transcript tail: {}",
                self.paths.present(&tail).replace('\n', "\\n")
            )
        }
    }
}

struct Recording {
    config: TapeConfig,
    paths: PathPresentation,
    path_filter: IncrementalPathPresentation,
    writer: Box<dyn Write + Send>,
    reader: Receiver<Vec<u8>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    marker: String,
    pending: Vec<u8>,
    text_pending: Vec<u8>,
    echoed: VecDeque<u8>,
    transcript: VecDeque<u8>,
    output_bytes: usize,
    ready_status: Option<i32>,
    screen: VirtualScreen,
    frames: Vec<TerminalFrame>,
    frame_overflow: bool,
    clock_ms: u32,
}

impl Recording {
    fn start(config: &TapeConfig, workspace_root: &Path) -> Result<Self, String> {
        let paths = PathPresentation::new(workspace_root, &config.path_root)?;
        let path_filter = IncrementalPathPresentation::new(paths.clone());
        let shell = choose_shell()?;
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize {
                rows: config.rows,
                cols: config.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| error.to_string())?;
        let mut command = shell.command();
        command.cwd(workspace_root);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        command.env("CLICOLOR_FORCE", "1");
        command.env("FORCE_COLOR", "1");
        command.env("PAGER", "cat");
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| error.to_string())?;
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| error.to_string())?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| error.to_string())?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || pump_reader(reader, sender));

        let marker = format!(
            "__EVIN_SENTINEL_{:x}_{:x}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        );
        let mut recording = Self {
            config: config.clone(),
            paths,
            path_filter,
            writer,
            reader: receiver,
            child,
            marker,
            pending: Vec::new(),
            text_pending: Vec::new(),
            echoed: VecDeque::new(),
            transcript: VecDeque::new(),
            output_bytes: 0,
            ready_status: None,
            screen: VirtualScreen::new(config.rows, config.cols),
            frames: Vec::new(),
            frame_overflow: false,
            clock_ms: 0,
        };
        recording.write_raw(shell.setup(&recording.marker).as_bytes())?;
        let _ = recording.wait_for_ready()?;
        recording.screen = VirtualScreen::new(config.rows, config.cols);
        recording.screen.write_str(shell.prompt());
        recording.capture();
        Ok(recording)
    }

    fn run(mut self, script: &TapeScript) -> Result<String, String> {
        let mut needs_wait = false;
        for command in &script.commands {
            match command {
                TapeCommand::Type(value) => {
                    self.type_text(value)?;
                    needs_wait = true;
                }
                TapeCommand::Backspace(count) => {
                    for _ in 0..*count {
                        self.write_raw(b"\x08")?;
                        self.echoed.push_back(0x08);
                        self.screen.write_str("\x08");
                        self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
                        self.capture();
                    }
                    needs_wait = true;
                }
                TapeCommand::Ctrl(letter) => {
                    let control = letter.to_ascii_uppercase() as u8 - b'@';
                    self.write_raw(&[control])?;
                    self.echoed.push_back(control);
                    self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
                    self.capture();
                    needs_wait = true;
                }
                TapeCommand::Enter => {
                    self.write_raw(b"\r")?;
                    self.echoed.extend(*b"\r\n");
                    self.screen.write_str("\r\n");
                    self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
                    self.capture();
                    needs_wait = true;
                }
                TapeCommand::Sleep(duration) => {
                    self.clock_ms = self.clock_ms.saturating_add(*duration);
                    self.capture();
                }
                TapeCommand::Expect(expected) => {
                    self.wait_for_expect(expected)?;
                    self.capture();
                }
                TapeCommand::Wait => {
                    let status = self.wait_for_ready()?;
                    if status != 0 {
                        return Err(
                            self.failure_with_tail(&format!("command exited with status {status}"))
                        );
                    }
                    self.clock_ms = self.clock_ms.saturating_add(self.config.command_settle_ms);
                    self.capture();
                    needs_wait = false;
                }
            }
        }
        if needs_wait {
            let status = self.wait_for_ready()?;
            if status != 0 {
                return Err(self.failure_with_tail(&format!("command exited with status {status}")));
            }
            self.clock_ms = self.clock_ms.saturating_add(self.config.command_settle_ms);
            self.capture();
        }
        self.flush_text()?;
        self.flush_path_filter();
        self.capture();
        if self.frame_overflow {
            return Err(self.failure_with_tail("terminal recording exceeded max-frames"));
        }
        Ok(render_svg(
            &self.config,
            &deduplicate(&self.frames),
            "A recorded terminal session. The final frame remains visible when reduced motion is enabled.",
        ))
    }

    fn type_text(&mut self, value: &str) -> Result<(), String> {
        self.write_raw(value.as_bytes())?;
        self.echoed.extend(value.bytes());
        for (character, color) in semantic_characters(value) {
            self.screen.write_styled_char(character, color);
            self.clock_ms = self.clock_ms.saturating_add(self.config.speed_ms);
            self.capture();
        }
        Ok(())
    }

    fn write_raw(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.writer
            .write_all(bytes)
            .map_err(|error| error.to_string())?;
        self.writer.flush().map_err(|error| error.to_string())
    }

    fn wait_for_ready(&mut self) -> Result<i32, String> {
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        loop {
            if let Some(status) = self.take_marker()? {
                self.flush_path_filter();
                return Ok(status);
            }
            let bytes = self.receive(deadline)?;
            self.pending.extend(bytes);
        }
    }

    fn wait_for_expect(&mut self, expected: &str) -> Result<(), String> {
        let deadline = Instant::now() + Duration::from_millis(self.config.timeout_ms);
        loop {
            self.consume_markers()?;
            self.flush_text()?;
            if self.ready_status.is_some() {
                self.flush_path_filter();
            }
            if self.screen.plain_text().contains(expected) {
                return Ok(());
            }
            let bytes = self.receive(deadline)?;
            self.pending.extend(bytes);
        }
    }

    fn receive(&mut self, deadline: Instant) -> Result<Vec<u8>, String> {
        let now = Instant::now();
        if now >= deadline {
            return Err(self.failure_with_tail("command timed out"));
        }
        self.reader
            .recv_timeout(deadline - now)
            .map_err(|_| self.failure_with_tail("shell ended or command timed out"))
    }

    fn take_marker(&mut self) -> Result<Option<i32>, String> {
        self.consume_markers()?;
        Ok(self.ready_status.take())
    }

    fn consume_markers(&mut self) -> Result<(), String> {
        let marker_prefix = format!("\x1e{}", self.marker).into_bytes();
        let Some(start) = find_bytes(&self.pending, &marker_prefix) else {
            let retain = marker_prefix.len().saturating_sub(1);
            let split = self.pending.len().saturating_sub(retain);
            if split > 0 {
                let visible: Vec<_> = self.pending.drain(..split).collect();
                self.feed_visible(&visible)?;
            }
            return Ok(());
        };
        if start > 0 {
            let visible: Vec<_> = self.pending.drain(..start).collect();
            self.feed_visible(&visible)?;
        }
        let Some(end_offset) = self.pending.iter().position(|byte| *byte == 0x1f) else {
            return Ok(());
        };
        let marker: Vec<_> = self.pending.drain(..=end_offset).collect();
        let text = std::str::from_utf8(&marker)
            .map_err(|_| self.failure_with_tail("malformed terminal sentinel"))?;
        let status = text
            .strip_prefix('\x1e')
            .and_then(|value| value.strip_prefix(&self.marker))
            .and_then(|value| value.strip_prefix('_'))
            .and_then(|value| value.strip_suffix("__\x1f"))
            .ok_or_else(|| self.failure_with_tail("malformed terminal sentinel"))?
            .parse::<i32>()
            .map_err(|_| self.failure_with_tail("malformed terminal sentinel"))?;
        self.ready_status = Some(status);
        Ok(())
    }

    fn feed_visible(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.output_bytes = self.output_bytes.saturating_add(bytes.len());
        if self.output_bytes > self.config.max_output_bytes {
            return Err(self.failure_with_tail("terminal output exceeded max-output"));
        }
        for byte in bytes {
            self.transcript.push_back(*byte);
            while self.transcript.len() > TRANSCRIPT_LIMIT {
                self.transcript.pop_front();
            }
            if self.echoed.front() == Some(byte) {
                self.echoed.pop_front();
            } else {
                self.echoed.clear();
                self.text_pending.push(*byte);
            }
        }
        self.flush_text()
    }

    fn flush_text(&mut self) -> Result<(), String> {
        if self.text_pending.is_empty() {
            return Ok(());
        }
        match std::str::from_utf8(&self.text_pending) {
            Ok(text) => {
                let presented = self.path_filter.push(text);
                self.screen.write_str(&presented);
                self.text_pending.clear();
            }
            Err(error) if error.error_len().is_some() => {
                return Err(self.failure_with_tail("malformed UTF-8 terminal stream"));
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    let text = std::str::from_utf8(&self.text_pending[..valid])
                        .map_err(|_| self.failure_with_tail("malformed UTF-8 terminal stream"))?;
                    let presented = self.path_filter.push(text);
                    self.screen.write_str(&presented);
                    self.text_pending.drain(..valid);
                }
            }
        }
        Ok(())
    }

    fn flush_path_filter(&mut self) {
        let presented = self.path_filter.finish();
        self.screen.write_str(&presented);
    }

    fn capture(&mut self) {
        let frame = self.screen.capture(self.clock_ms);
        if self.frames.last().is_some_and(|previous| {
            previous.cells == frame.cells
                && previous.cursor_row == frame.cursor_row
                && previous.cursor_col == frame.cursor_col
        }) {
            if let Some(previous) = self.frames.last_mut() {
                previous.timestamp_ms = frame.timestamp_ms;
            }
        } else {
            self.frames.push(frame);
        }
        if self.frames.len() > self.config.max_frames {
            self.frame_overflow = true;
            self.frames.truncate(self.config.max_frames);
        }
    }

    fn failure_with_tail(&self, message: &str) -> String {
        let tail_bytes = self.transcript.iter().copied().collect::<Vec<_>>();
        let tail = String::from_utf8_lossy(&tail_bytes);
        if tail.trim().is_empty() {
            message.to_string()
        } else {
            format!(
                "{message}; transcript tail: {}",
                self.paths.present(&tail).replace('\n', "\\n")
            )
        }
    }
}

impl Drop for Recording {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn pump_reader(mut reader: Box<dyn Read + Send>, sender: mpsc::Sender<Vec<u8>>) {
    let mut buffer = [0u8; 4096];
    while let Ok(count) = reader.read(&mut buffer) {
        if count == 0 {
            break;
        }
        if sender.send(buffer[..count].to_vec()).is_err() {
            break;
        }
    }
}

fn choose_shell() -> Result<Shell, String> {
    #[cfg(unix)]
    {
        if command_available("bash") {
            return Ok(Shell::Bash);
        }
        if command_available("zsh") {
            return Ok(Shell::Zsh);
        }
        return Err("no supported profile-free shell found (need bash or zsh)".to_string());
    }
    #[cfg(windows)]
    {
        if command_available("powershell.exe") {
            return Ok(Shell::PowerShell);
        }
        if command_available("cmd.exe") {
            return Ok(Shell::Cmd);
        }
        return Err("no supported shell found (need PowerShell or CMD)".to_string());
    }
    #[allow(unreachable_code)]
    Err("terminal tapes are unsupported on this host".to_string())
}

fn command_available(command: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|path| path.join(command).is_file()))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn deduplicate(frames: &[TerminalFrame]) -> Vec<TerminalFrame> {
    let mut result: Vec<TerminalFrame> = Vec::new();
    for frame in frames {
        if let Some(previous) = result.last_mut()
            && previous.cells == frame.cells
            && previous.cursor_row == frame.cursor_row
            && previous.cursor_col == frame.cursor_col
        {
            previous.timestamp_ms = frame.timestamp_ms;
        } else {
            result.push(frame.clone());
        }
    }
    result
}

/// ESH owns language classification. The terminal package only receives the
/// neutral semantic categories it needs to render a typed character.
fn semantic_characters(source: &str) -> Vec<(char, TerminalColor)> {
    let mut result = Vec::with_capacity(source.chars().count());
    for (line_index, segment) in source.split_inclusive('\n').enumerate() {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let tokens = scan_tokens(line, line_index);
        let classified = tokens.iter().map(|token| token.text).collect::<String>();
        if classified == line {
            for token in tokens {
                let color = token_color(token.kind);
                result.extend(
                    token
                        .text
                        .chars()
                        .map(|character| (character, color.clone())),
                );
            }
        } else {
            result.extend(
                line.chars()
                    .map(|character| (character, TerminalColor::Default)),
            );
        }
        if segment.ends_with('\n') {
            result.push(('\n', TerminalColor::Default));
        }
    }
    if source.is_empty() {
        return result;
    }
    result
}

fn token_color(kind: TokenKind) -> TerminalColor {
    match kind {
        TokenKind::Plain => TerminalColor::Default,
        TokenKind::Comment => TerminalColor::Comment,
        TokenKind::Keyword => TerminalColor::Keyword,
        TokenKind::Command => TerminalColor::Command,
        TokenKind::Builtin => TerminalColor::Builtin,
        TokenKind::Flag => TerminalColor::Flag,
        TokenKind::Variable => TerminalColor::Variable,
        TokenKind::String => TerminalColor::String,
        TokenKind::Number => TerminalColor::Number,
        TokenKind::Operator => TerminalColor::Operator,
        TokenKind::Punctuation => TerminalColor::Punctuation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_a_persistent_system_shell_command() {
        let script = TapeScript::parse("type \"cd /tmp\"\nenter\nwait\ntype \"pwd\"\nenter\nwait")
            .expect("valid tape script");
        let config = TapeConfig {
            runtime: TapeRuntime::System,
            ..TapeConfig::default()
        };
        let recording = Recording::start(&config, Path::new(".")).expect("supported shell and PTY");
        let svg = recording.run(&script).expect("successful tape recording");
        let final_frame = svg.rsplit("<g class=\"tape-frame").next().unwrap();
        assert!(svg.contains("/tmp"));
        assert!(svg.contains("term-command"));
        assert_eq!(final_frame.matches(">cd</tspan>").count(), 1);
        assert!(svg.contains("data-tape-duration-ms"));
    }

    #[test]
    fn system_pty_normalizes_workspace_pwd_output() {
        let script = TapeScript::parse("type \"pwd\"\nenter\nwait").unwrap();
        let config = TapeConfig {
            runtime: TapeRuntime::System,
            path_root: "/docs".to_string(),
            ..TapeConfig::default()
        };
        let svg = Recording::start(&config, Path::new("."))
            .unwrap()
            .run(&script)
            .unwrap();
        let workspace = Path::new(".").canonicalize().unwrap().display().to_string();

        assert!(svg.contains("/docs"));
        assert!(!svg.contains(&workspace));
    }

    #[test]
    fn presents_workspace_paths_without_rewriting_external_paths_or_ansi() {
        let workspace = tempfile::tempdir().unwrap();
        let presentation = PathPresentation::new(workspace.path(), "/docs").unwrap();
        let physical = workspace
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let rendered =
            presentation.present(&format!("\x1b[31m{physical}/src/lib.rs\x1b[0m /tmp/file"));
        assert_eq!(rendered, "\x1b[31m/docs/src/lib.rs\x1b[0m /tmp/file");

        let split = physical.len() / 2;
        let mut incremental = IncrementalPathPresentation::new(presentation);
        assert!(incremental.push(&physical[..split]).is_empty());
        let rendered = format!(
            "{}{}",
            incremental.push(&physical[split..]),
            incremental.finish()
        );
        assert_eq!(rendered, "/docs");
    }

    #[test]
    fn records_a_persistent_esh_session_and_normalizes_workspace_output() {
        let script = TapeScript::parse("type \"pwd\"\nenter\nwait").expect("valid tape script");
        let svg = run_esh_tape(&TapeConfig::default(), Path::new("."), &script)
            .expect("successful ESH tape recording");
        let workspace = Path::new(".").canonicalize().unwrap().display().to_string();

        assert!(svg.contains("/workspace"));
        assert!(!svg.contains(&workspace));
        assert!(svg.contains("term-builtin"));
        assert!(svg.contains("data-tape-duration-ms"));
    }

    #[test]
    fn esh_tapes_expose_a_color_capable_virtual_terminal() {
        let recording = EshRecording::start(TapeConfig::default(), Path::new(".").to_path_buf())
            .expect("recording starts");

        assert_eq!(
            recording.context.env_vars().get("EVIN_TAPE"),
            Some(&"1".to_string())
        );
        assert_eq!(
            recording.context.env_vars().get("TERM"),
            Some(&"xterm-256color".to_string())
        );
        assert_eq!(
            recording.context.env_vars().get("COLORTERM"),
            Some(&"truecolor".to_string())
        );
    }

    #[test]
    fn supports_a_custom_workspace_display_root() {
        let script = TapeScript::parse("type \"pwd\"\nenter\nwait").unwrap();
        let config = TapeConfig {
            path_root: "/docs".to_string(),
            ..TapeConfig::default()
        };
        let svg = run_esh_tape(&config, Path::new("."), &script).unwrap();
        assert!(svg.contains("/docs"));
        assert!(!svg.contains("/workspace"));
    }

    #[test]
    fn maps_every_esh_token_category_and_preserves_unicode_input() {
        assert_eq!(token_color(TokenKind::Plain), TerminalColor::Default);
        assert_eq!(token_color(TokenKind::Comment), TerminalColor::Comment);
        assert_eq!(token_color(TokenKind::Keyword), TerminalColor::Keyword);
        assert_eq!(token_color(TokenKind::Command), TerminalColor::Command);
        assert_eq!(token_color(TokenKind::Builtin), TerminalColor::Builtin);
        assert_eq!(token_color(TokenKind::Flag), TerminalColor::Flag);
        assert_eq!(token_color(TokenKind::Variable), TerminalColor::Variable);
        assert_eq!(token_color(TokenKind::String), TerminalColor::String);
        assert_eq!(token_color(TokenKind::Number), TerminalColor::Number);
        assert_eq!(token_color(TokenKind::Operator), TerminalColor::Operator);
        assert_eq!(
            token_color(TokenKind::Punctuation),
            TerminalColor::Punctuation
        );

        let typed = semantic_characters("echo \"λ\"");
        assert_eq!(
            typed
                .iter()
                .map(|(character, _)| *character)
                .collect::<String>(),
            "echo \"λ\""
        );
        assert!(
            typed
                .iter()
                .any(|(_, color)| *color == TerminalColor::String)
        );
    }

    #[test]
    fn esh_expect_does_not_wait_for_interactive_prompts() {
        let script =
            TapeScript::parse("type \"printf tape-ok\"\nenter\nexpect \"missing\"").unwrap();
        let error = run_esh_tape(&TapeConfig::default(), Path::new("."), &script).unwrap_err();
        assert!(
            error.contains("cannot wait for interactive prompts"),
            "unexpected error: {error}"
        );
    }
}
