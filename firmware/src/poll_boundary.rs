//! Non-inlined async poll boundaries for the memory-constrained service core.

pin_project_lite::pin_project! {
    /// Prevents LLVM from merging several mutually exclusive async phase
    /// implementations into one maximum-sized native stack frame.
    pub(crate) struct PollBoundary<F> {
        #[pin]
        inner: F,
    }
}

impl<F: core::future::Future> core::future::Future for PollBoundary<F> {
    type Output = F::Output;

    #[inline(never)]
    fn poll(
        self: core::pin::Pin<&mut Self>,
        context: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        core::future::Future::poll(self.project().inner, context)
    }
}

pub(crate) fn poll_boundary<F: core::future::Future>(inner: F) -> PollBoundary<F> {
    PollBoundary { inner }
}
