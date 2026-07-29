import { useEffect, useState } from "react";

import { getConsoleImageWidths, type ConsoleImageWidths } from "../lib/consoleImageSizing";

function read(): ConsoleImageWidths {
  return getConsoleImageWidths(window.innerWidth, window.devicePixelRatio);
}

/**
 * Request widths for console artwork, tracking the window size.
 *
 * State only changes when a value crosses a ladder step, so a drag-resize does
 * not re-request every image on the way — and the URLs stay cache-stable.
 */
export function useConsoleImageWidths(): ConsoleImageWidths {
  const [widths, setWidths] = useState<ConsoleImageWidths>(read);

  useEffect(() => {
    const handleResize = (): void => {
      setWidths((current) => {
        const next = read();
        const unchanged = current.card === next.card
          && current.billboard === next.billboard
          && current.screenshot === next.screenshot
          && current.thumb === next.thumb;
        return unchanged ? current : next;
      });
    };

    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  return widths;
}
