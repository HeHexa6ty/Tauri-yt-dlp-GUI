const SUPPORTED_HOSTS = [
  "youtube.com",
  "youtu.be",
  "instagram.com",
  "x.com",
  "twitter.com",
  "tiktok.com",
];

/** Adds a scheme when the user pastes a bare domain like `youtube.com/watch?v=…`. */
export function normalizeUrlInput(raw: string): string {
  const value = raw.trim();
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(value) ? value : `https://${value}`;
}

/**
 * Returns true when the input points at a supported host or one of its
 * subdomains. Parsing avoids false positives such as `youtube.com.evil.com`.
 */
export function isSupportedUrl(raw: string): boolean {
  try {
    const host = new URL(normalizeUrlInput(raw)).hostname
      .toLowerCase()
      .replace(/^www\./, "");

    return SUPPORTED_HOSTS.some(
      (supported) => host === supported || host.endsWith(`.${supported}`),
    );
  } catch {
    return false;
  }
}
