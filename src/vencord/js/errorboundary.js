// @components/ErrorBoundary: plugins wrap their components in it; Rustcord renders them bare.
const ErrorBoundary = Object.assign(p => p.children ?? null, { wrap: c => c });
export default ErrorBoundary;
