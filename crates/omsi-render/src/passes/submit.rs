//! The frame's command buffers finished (side by side when there is much to finish) and
//! submitted.

use super::*;

impl Renderer {
    /// Finish the encoders and submit them: the shadow maps, the prepass, the main pass's
    /// parts, then the picture. `big`: finished on helper threads.
    pub(crate) fn finish_and_submit(&mut self, encoders: Encoders, main_parts: Vec<wgpu::CommandEncoder>, big: bool, with_overlays: bool, clock: &mut StageClock, timers: PassTimers) {
        let Encoders { shadow: shadow_encoder, prepass: prepass_encoder, picture: encoder } = encoders;
        let profiling = self.profiling;
        let finish = |encoder: wgpu::CommandEncoder| {
            let start = profiling.then(std::time::Instant::now);
            let commands = encoder.finish();
            (commands, start.map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0))
        };
        let (shadow_commands, prepass_commands, part_commands, commands, finish_times) = if let (true, Some(pool)) = (big, self.encoding_pool.as_ref()) {
            // Every encoder is finished by whoever is free, this thread too: it no longer waits
            // for a helper the system has not run yet to finish the one it was given.
            let caller = std::thread::current().id();
            let parts = main_parts.len();
            let jobs: Vec<std::sync::Mutex<Option<wgpu::CommandEncoder>>> = [encoder, shadow_encoder, prepass_encoder]
                .into_iter()
                .chain(main_parts)
                .map(|e| std::sync::Mutex::new(Some(e)))
                .collect();
            let start = std::time::Instant::now();
            let done = run_parts(Some(pool), jobs.len(), |k| {
                let e = jobs[k].lock().unwrap_or_else(|e| e.into_inner()).take().expect("an encoder finished twice");
                let (c, secs) = finish(e);
                (c, secs, std::thread::current().id() == caller)
            });
            // (what this thread spent waiting: the whole less the encoders it finished itself)
            let own: f64 = done.iter().filter(|d| d.2).map(|d| d.1).sum();
            let wait = if profiling { (start.elapsed().as_secs_f64() - own).max(0.0) } else { 0.0 };
            let mut done = done.into_iter();
            let (commands, main_secs, _) = done.next().expect("the picture's encoder");
            let (shadow_commands, shadow_secs, _) = done.next().expect("the shadows' encoder");
            let (prepass_commands, prepass_secs, _) = done.next().expect("the prepass's encoder");
            let (part_commands, part_secs): (Vec<wgpu::CommandBuffer>, Vec<f64>) = done.map(|d| (d.0, d.1)).unzip();
            debug_assert_eq!(part_commands.len(), parts);
            (shadow_commands, prepass_commands, part_commands, commands, [shadow_secs, prepass_secs, main_secs, wait, 0.0, part_secs.iter().sum()])
        } else if big {
            std::thread::scope(|scope| {
                let finish = &finish;
                let shadow = scope.spawn(move || finish(shadow_encoder));
                let prepass = scope.spawn(move || finish(prepass_encoder));
                let parts: Vec<_> = main_parts.into_iter().map(|e| scope.spawn(move || finish(e))).collect();
                let (commands, main_secs) = finish(encoder);
                let wait = std::time::Instant::now();
                let (shadow_commands, shadow_secs) = shadow.join().expect("command encoding thread");
                let shadow_wait = wait.elapsed().as_secs_f64();
                let wait = std::time::Instant::now();
                let (prepass_commands, prepass_secs) = prepass.join().expect("command encoding thread");
                let (part_commands, part_secs): (Vec<_>, Vec<f64>) = parts.into_iter().map(|h| h.join().expect("command encoding thread")).unzip();
                let prepass_wait = wait.elapsed().as_secs_f64();
                (
                    shadow_commands,
                    prepass_commands,
                    part_commands,
                    commands,
                    [shadow_secs, prepass_secs, main_secs, shadow_wait, prepass_wait, part_secs.iter().sum()],
                )
            })
        } else {
            let (shadow, shadow_secs) = finish(shadow_encoder);
            let (prepass, prepass_secs) = finish(prepass_encoder);
            let (parts, part_secs): (Vec<_>, Vec<f64>) = main_parts.into_iter().map(finish).unzip();
            let (main, main_secs) = finish(encoder);
            (shadow, prepass, parts, main, [shadow_secs, prepass_secs, main_secs, 0.0, 0.0, part_secs.iter().sum()])
        };
        if self.profiling {
            // These overlap across helper threads; do not add them to the stage totals.
            let keys = if with_overlays {
                ["finish.shadow", "finish.prepass", "finish.main", "finish.wait", "finish.wait prepass", "finish.parts"]
            } else {
                ["mirror.finish.shadow", "mirror.finish.prepass", "mirror.finish.main", "mirror.finish.wait", "mirror.finish.wait prepass", "mirror.finish.parts"]
            };
            for (key, secs) in keys.into_iter().zip(finish_times) {
                *self.stats.borrow_mut().entry(key).or_default() += secs;
            }
        }
        clock.stage(self, "finish", "mirror.finish");
        self.queue
            .submit([shadow_commands, prepass_commands].into_iter().chain(part_commands).chain([commands]));
        clock.stage(self, "submit", "mirror.submit");
        let timed = timers.timed;
        if let (Some(t), false) = (
            self.gpu_timers[with_overlays as usize].as_mut(),
            timed.is_empty(),
        ) {
            t.pending = timed;
            t.unresolved = true;
        }
    }
}
