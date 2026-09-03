(function() {
    'use strict';

    window.MatterActions = window.MatterActions || window.DocsActions || window.EvinActions || {};
    window.DocsActions = window.MatterActions;
    window.EvinActions = window.MatterActions;
    const tapePlayers = window.MatterTapePlayers || (window.MatterTapePlayers = new WeakMap());
    window.EvinTapePlayers = tapePlayers;

    function createTapePlayer(tape) {
        const svg = tape.querySelector('.terminal-tape-svg[data-tape-duration-ms], .terminal-tape[data-tape-duration-ms], svg[data-tape-duration-ms]');
        const duration = Number(svg?.dataset.tapeDurationMs);
        const frames = [...(svg?.querySelectorAll('.tape-frame') || [])].map((frame) => ({
            element: frame,
            start: Number(frame.dataset.tapeFrameStartMs),
        }));
        if (!svg || !Number.isFinite(duration) || duration <= 0 || frames.length === 0) return null;

        let animationFrame = 0;
        let paused = false;
        let elapsed = 0;
        let startedAt = performance.now();
        let activeFrame = -1;

        const frameIndexAt = (time) => {
            const loopTime = time % duration;
            for (let index = frames.length - 1; index >= 0; index -= 1) {
                if (frames[index].start <= loopTime) return index;
            }
            return 0;
        };

        const showFrame = (index) => {
            if (index === activeFrame) return;
            if (activeFrame >= 0 && frames[activeFrame]) {
                frames[activeFrame].element.style.display = 'none';
            }
            frames[index].element.style.display = 'inline';
            activeFrame = index;
            svg.dataset.activeFrame = String(index);
        };

        const tick = (now) => {
            if (paused) return;
            elapsed = now - startedAt;
            showFrame(frameIndexAt(elapsed));
            animationFrame = requestAnimationFrame(tick);
        };

        // Hide static fallback frames (the last frame is visible in static HTML)
        for (let i = 1; i < frames.length; i++) {
            frames[i].element.style.display = 'none';
        }
        frames[0].element.style.display = 'inline';
        activeFrame = 0;
        svg.dataset.activeFrame = '0';
        animationFrame = requestAnimationFrame(tick);
        return {
            setPaused(nextPaused) {
                if (paused === nextPaused) return;
                paused = nextPaused;
                if (paused) {
                    cancelAnimationFrame(animationFrame);
                } else {
                    startedAt = performance.now() - elapsed;
                    animationFrame = requestAnimationFrame(tick);
                }
            },
        };
    }

    function setPlaybackState(button, paused) {
        const tape = button.closest('.terminal-tape');
        if (!tape) return;

        tapePlayers.get(tape)?.setPaused(paused);
        tape.classList.toggle('is-paused', paused);
        button.setAttribute('aria-pressed', String(paused));
        const label = paused ? 'Resume playback' : 'Pause playback';
        button.setAttribute('aria-label', label);
        button.setAttribute('title', label);
    }

    window.MatterActions['tape:toggle-playback'] = function(button, event) {
        if (button.disabled) return;
        const tape = button.closest('.terminal-tape');
        setPlaybackState(button, !tape?.classList.contains('is-paused'));
    };

    window.MatterActions['tape:copy'] = function(button, event) {
        if (button.disabled) return;
        const tape = button.closest('.terminal-tape');
        const payload = tape?.querySelector('.tape-copy-script');
        const script = payload?.content.textContent || '';
        if (!script) return;

        if (window.MatterActions.copyText) {
            window.MatterActions.copyText(script, button, 'Copied!');
        }
    };

    function initializePlayback() {
        if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) {
            document.querySelectorAll('.tape-playback').forEach(function(button) {
                button.disabled = true;
                button.setAttribute('aria-label', 'Playback disabled because reduced motion is enabled');
                button.setAttribute('title', 'Playback disabled because reduced motion is enabled');
            });
            return;
        }
        document.querySelectorAll('.terminal-tape').forEach(function(tape) {
            if (tapePlayers.has(tape)) return;
            const player = createTapePlayer(tape);
            if (player) tapePlayers.set(tape, player);
        });
    }

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initializePlayback);
    } else {
        initializePlayback();
    }
})();
