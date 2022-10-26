pub struct Stats {
    span: tracing_impl::Span,
}

impl Stats {
    pub fn new() -> Self {
        let span = tracing_impl::trace_span!(
            "runtime.resource",
            concrete_type = "Inotify",
            kind = "file",
            is_internal = false,
            inherits_child_attrs = true,
        );

        span.in_scope(|| {
            tracing_impl::trace!(
                target: "runtime::resource::state_update",
                watching = 0,
                watching.unit = "files",
                watching.op = "override"
            );

            tracing_impl::trace!(
                target: "runtime::resource::state_update",
                events = 0,
                events.unit = "events",
                events.op = "override"
            );
        });

        Self { span }
    }

    pub fn inc_files(&self, count: usize) {
        self.span.in_scope(|| {
            tracing_impl::trace!(
                 target: "runtime::resource::state_update",
                 watching = count,
                 watching.unit = "files",
                 watching.op = "add"
            );
        });
    }

    pub fn dec_files(&self, count: usize) {
        self.span.in_scope(|| {
            tracing_impl::trace!(
                 target: "runtime::resource::state_update",
                 watching = count,
                 watching.unit = "files",
                 watching.op = "sub"
            );
        });
    }

    pub fn note_events(&self, count: usize) {
        tracing_impl::trace!(
            target: "runtime::resource::state_update",
            events = count,
            events.unit = "events",
            events.op = "add"
        );
    }
}

