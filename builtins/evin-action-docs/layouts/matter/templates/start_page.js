(function() {
    'use strict';

    /* ==========================================================================
       HERO PARTICLE PARALLAX CONTROLLER (Start Page Component Module)
       ========================================================================== */
    const initHeroParticles = () => {
        const hero = document.querySelector('.start-hero');
        if (!hero) return;

        // Check user preference for reduced motion
        const prefersReducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
        if (prefersReducedMotion) return;

        let targetX = 0;
        let targetY = 0;
        let currentX = 0;
        let currentY = 0;
        let isRunning = false;
        let isHeroVisible = true;

        const LERP_FACTOR = 0.075; // Silky smooth damping factor
        const EPSILON = 0.0001;

        const updateParallax = () => {
            if (!isHeroVisible) {
                isRunning = false;
                return;
            }

            // Smooth linear interpolation (LERP) towards target coordinates
            currentX += (targetX - currentX) * LERP_FACTOR;
            currentY += (targetY - currentY) * LERP_FACTOR;

            hero.style.setProperty('--mouse-x', currentX.toFixed(4));
            hero.style.setProperty('--mouse-y', currentY.toFixed(4));

            // Keep animating until current values converge to target
            const deltaX = Math.abs(targetX - currentX);
            const deltaY = Math.abs(targetY - currentY);

            if (deltaX > EPSILON || deltaY > EPSILON) {
                requestAnimationFrame(updateParallax);
            } else {
                currentX = targetX;
                currentY = targetY;
                hero.style.setProperty('--mouse-x', currentX.toFixed(4));
                hero.style.setProperty('--mouse-y', currentY.toFixed(4));
                isRunning = false;
            }
        };

        const startLoop = () => {
            if (!isRunning && isHeroVisible) {
                isRunning = true;
                requestAnimationFrame(updateParallax);
            }
        };

        // Track visibility of hero section via IntersectionObserver
        if ('IntersectionObserver' in window) {
            const observer = new IntersectionObserver((entries) => {
                for (const entry of entries) {
                    isHeroVisible = entry.isIntersecting;
                    if (isHeroVisible && (Math.abs(targetX - currentX) > EPSILON || Math.abs(targetY - currentY) > EPSILON)) {
                        startLoop();
                    }
                }
            }, { threshold: 0.05 });

            observer.observe(hero);
        }

        window.addEventListener('pointermove', (e) => {
            if (!isHeroVisible) return;

            // Normalized coordinate from -1.0 to 1.0 based on viewport center
            targetX = ((e.clientX / window.innerWidth) - 0.5) * 2;
            targetY = ((e.clientY / window.innerHeight) - 0.5) * 2;

            startLoop();
        }, { passive: true });

        // Smoothly settle back to center when pointer leaves the window
        document.addEventListener('mouseleave', () => {
            targetX = 0;
            targetY = 0;
            startLoop();
        }, { passive: true });
    };

    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', initHeroParticles);
    } else {
        initHeroParticles();
    }
})();
