// @components/ErrorBoundary: plugins wrap their components in it; Fastcord renders them bare.
const ErrorBoundary = Object.assign(p => p.children ?? null, { wrap: c => c });
export default ErrorBoundary;
