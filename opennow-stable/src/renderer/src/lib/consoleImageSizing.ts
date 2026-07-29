/**
 * Right-sizing for NVIDIA catalog artwork.
 *
 * The catalog hands out URLs pinned to `;w=1200` — roughly 200 KB per image —
 * even though a console card renders at about 180 CSS px. A single store page
 * is ~325 cards, so the shell was pulling tens of megabytes of art it then
 * downscaled, which also evicted its own images from the HTTP cache and made
 * every launch re-download everything.
 *
 * The CDN resizes on demand via the `;w=` parameter and serves
 * `Cache-Control: max-age=604800`, so asking for the size actually needed both
 * cuts the transfer and lets a whole catalog stay cached between launches.
 */

/**
 * Requests snap to this ladder rather than using an exact pixel width. Exact
 * widths would mint a unique URL per window size and miss the cache on every
 * resize; a ladder keeps a handful of reusable URLs.
 */
export const IMAGE_WIDTH_LADDER = [160, 240, 320, 480, 640, 960, 1280, 1920] as const;

const RESIZABLE_HOST = "img.nvidiagrid.net";

export function snapImageWidth(px: number): number {
  if (!Number.isFinite(px) || px <= 0) return IMAGE_WIDTH_LADDER[0];
  return IMAGE_WIDTH_LADDER.find((step) => step >= px) ?? IMAGE_WIDTH_LADDER[IMAGE_WIDTH_LADDER.length - 1];
}

/**
 * Rewrites the CDN width parameter. Leaves untouched any URL that is not a
 * resizable NVIDIA asset — Steam CDN URLs, for instance, would 404 if given
 * these parameters.
 */
export function withImageWidth(url: string | undefined, px: number): string | undefined {
  if (!url) return url;

  const isResizable = url.includes(RESIZABLE_HOST) || /;w=\d+/.test(url);
  if (!isResizable) return url;

  const width = snapImageWidth(px);
  if (/;w=\d+/.test(url)) return url.replace(/;w=\d+/, `;w=${width}`);
  if (/;f=[a-z0-9]+/i.test(url)) return `${url};w=${width}`;
  return `${url};f=webp;w=${width}`;
}

export interface ConsoleImageWidths {
  /** Poster cards in a shelf. */
  card: number;
  /** Full-bleed billboard art. */
  billboard: number;
  /** Detail-sheet screenshot viewer. */
  screenshot: number;
  /** Detail-sheet screenshot thumbnails. */
  thumb: number;
}

/**
 * Console layout is proportional (see --console-u in styles/console.css), so
 * every element's rendered width is a fixed fraction of the viewport. That
 * makes the right request size derivable without measuring the DOM.
 */
export function getConsoleImageWidths(viewportWidth: number, devicePixelRatio = 1): ConsoleImageWidths {
  const dpr = Math.min(Math.max(devicePixelRatio, 1), 2);
  const px = (fraction: number): number => snapImageWidth(viewportWidth * fraction * dpr);
  return {
    card: px(0.094),
    billboard: px(1),
    screenshot: px(0.42),
    thumb: px(0.06),
  };
}
