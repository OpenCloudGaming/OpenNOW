const CURSOR_POSITION_MAX = 65535;

export interface GfnCursorPosition {
  x: number;
  y: number;
}

export interface GfnPredefinedCursorMessage {
  type: "predefined";
  cursorId: number;
  position?: GfnCursorPosition;
}

export interface GfnCustomCursorMessage {
  type: "custom";
  cursorId: number;
  hotspotX: number;
  hotspotY: number;
  mimeType: string;
  imageBase64: string;
  position?: GfnCursorPosition;
  scale: number;
}

export type GfnCursorChannelMessage = GfnPredefinedCursorMessage | GfnCustomCursorMessage;

interface GfnCursorShape {
  id: number;
  style: string;
  hotspotX: number;
  hotspotY: number;
  mimeType: string;
  imageBase64: string;
  scale: number;
  image?: HTMLImageElement;
  nativeStyle?: string;
}

interface StreamViewport {
  originX: number;
  originY: number;
  width: number;
  height: number;
}

const PREDEFINED_CURSORS: GfnCursorShape[] = [
  { id: 0, style: "none", hotspotX: 0, hotspotY: 0, mimeType: "image/x-icon", imageBase64: "", scale: 1 },
  { id: 1, style: "default", hotspotX: 2, hotspotY: 1, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAYAAAAOAAAADAAAABwAAAAYAAAAOAAABDAAAAZwAAAHYAAAB4AAAAf+AAAH/AAAB/gAAAfwAAAH4AAAB8AAAAeAAAAHAAAABgAAAAQAAAAAAAAAAAAAAAAAAAA////////////////////////////////////////////5////8P///+D////h////wf//98P///OD///xh///8Af///AP///wAH//8AD///AB///wA///8Af///AP///wH///8D////B////w////8f////P////3/////////8=", scale: 1 },
  { id: 2, style: "text", hotspotX: 8, hotspotY: 13, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAfvwAAAAAAAABAAAAAQAAAAEAAAABAAAAAQAAAAEAAAABAAAAAQAAAAEAAAABAAAAAQAAAAEAAAABAAAAAQAAAAEAAAABAAAAAQAAAAEAAAABAAAAfPwAAAAAAAAAAAAAAAAAA/////////////////////////////////////+BA///Agf///z////8/////P////z////8/////P////z////8/////P////z////8/////P////z////8/////P////z////8/////P////z///+BA///Agf////////////8=", scale: 1 },
  { id: 3, style: "wait", hotspotX: 7, hotspotY: 12, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD/8AAAzzAAAHDgAABfoAAAb2AAADbAAAAfgAAABgAAAAYAAAAGAAAABgAAAAYAAAAGAAAABgAAAB+AAAA/wAAAaWAAAH/gAABWoAAA//AAAP/wAAAAAAAAAAAAA//////////////////////////////////////////+AAf//gAH//4AB//+AAf//wAP//8AD///gB///8A////gf///8P////D////w////8P////D////gf///wD///4Af//8AD///AA///gAH//4AB//+AAf//gAH///////8=", scale: 1 },
  { id: 4, style: "crosshair", hotspotX: 8, hotspotY: 8, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABQAAABjAAABgMAAAQBAAAIAIAACACAABAAQAAAIAAAEABAAAgAgAAIAIAABgMAAAICAAABjAAAAFAAAAAAAAA//////////////////////////////////////////////////////////////////////////////////////4////5T///53P//+97///fff//333//79+//+AAP//v37//999///fff//53P///d3///5T////j////////8=", scale: 1 },
  { id: 5, style: "progress", hotspotX: 2, hotspotY: 1, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD4AAAAiAAAAPgAAABwAAAYIAAAOCAAADAgAABwcAAAYNgAAOCoABDA+AAZwAAAHYAAAB4AAAAf+AAAH/AAAB/gAAAfwAAAH4AAAB8AAAAeAAAAHAAAABgAAAAQAAAAAAAAAAAAAAAAAAAA///////////////////////+A////gP///4D///+A///5wf//8OP//+Dj///h4///wcH/98OA//ODgP/xh4D/8AeA//AP///wAH//8AD///AB///wA///8Af///AP///wH///8D////B////w////8f////P////3/////////8=", scale: 1 },
  { id: 6, style: "nwse-resize", hotspotX: 9, hotspotY: 8, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAP4AAAB+AAAAPgAAAB4AAABuAAAA5gAAAcIAAEOAAABnAAAAdgAAAHgAAAB8AAAAfgAAAH8AAAAAAAAAAAAAA/////////////////////////////////////////////////////////////////////////////////wA///+AP///wD///+A////gP///wD//34A//88GP//GDz//wB+//8A////Af///wH///8A////AH///wA////////8=", scale: 1 },
  { id: 7, style: "nesw-resize", hotspotX: 9, hotspotY: 9, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAfwAAAH4AAAB8AAAAeAAAAHYAAABnAAAAQ4AAAAHCAAAA5gAAAG4AAAAeAAAAPgAAAH4AAAD+AAAAAAAAAAAAA////////////////////////////////////////////////////////////////////////////////wA///8Af///AP///wH///8B////AP///wB+//8YPP//PBj//34A////AP///4D///+A////AP///gD///wA///////8=", scale: 1 },
  { id: 8, style: "ew-resize", hotspotX: 13, hotspotY: 8, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAEAABgBgAA4AcAAeAHgAPv98AH7/fgA+/3wAHgB4AA4AcAAGAGAAAgBAAAAAAAAAAAAAAAAAA//////////////////////////////////////////////////////////////////////////////////////+/3///P8///j/H//w/w//4P8H/8AAA/+AAAH/AAAA/4AAAf/AAAP/4P8H//D/D//4/x///P8///7/f//////8=", scale: 1 },
  { id: 9, style: "ns-resize", hotspotX: 9, hotspotY: 12, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAADgAAAB8AAAA/gAAAf8AAAP/gAAAAAAAADgAAAA4AAAAOAAAADgAAAA4AAAAOAAAADgAAAA4AAAAAAAAA/+AAAH/AAAA/gAAAHwAAAA4AAAAEAAAAAAAAAAAAAA//////////////////////////////////////+/////H////g////wH///4A///8AH//+AA///AAH///g////4P///+D////g////4P///+D////g////4P///AAH//4AD///AB///4A////Af///4P////H////7////////8=", scale: 1 },
  { id: 10, style: "move", hotspotX: 13, hotspotY: 12, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACAAAABwAAAA+AAAAHAAAABwAAAAcAAAAHAAAABwAAAgACAAf3fwAP93+AB/d/AAIAAgAABwAAAAcAAAAHAAAABwAAAAcAAAAPgAAABwAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/////////////////////////////f////j////wf///4D///8Af///wf///8H///vB7//zwef/4AAD/8AAAf+AAAD/wAAB/+AAA//zwef/+8Hv///B////wf///wB///+A////wf///+P////3///////////////////////8=", scale: 1 },
  { id: 11, style: "default", hotspotX: 2, hotspotY: 1, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAYAAAAOAAAADAAAABwAAAAYAAAAOAAABDAAAAZwAAAHYAAAB4AAAAf+AAAH/AAAB/gAAAfwAAAH4AAAB8AAAAeAAAAHAAAABgAAAAQAAAAAAAAAAAAAAAAAAAA////////////////////////////////////////////5////8P///+D////h////wf//98P///OD///xh///8Af///AP///wAH//8AD///AB///wA///8Af///AP///wH///8D////B////w////8f////P////3/////////8=", scale: 1 },
  { id: 12, style: "pointer", hotspotX: 8, hotspotY: 3, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAH+AAAB/gAAA/4AAAP/AAAH/wAAB/8AAA//gAAP/4AAH/+AADttgABzbYAAY2wAAANsAAADYAAAAwAAAAMAAAAAAAAAAAAAA//////////////////////////////////////////////////////////////////////+Af///AD///wA///4AP//+AB///AAf//wAH//4AA//+AAP//AAD//gAA//wAAP/8IAH//mAH///gD///4H///+H////z////////8=", scale: 1 },
  { id: 13, style: "help", hotspotX: 2, hotspotY: 1, mimeType: "image/x-icon", imageBase64: "AAABAAEAICACAAEAAQAwAQAAFgAAACgAAAAgAAAAQAAAAAEAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA////AAAAAAAAAAAAAAAAAAAAGAAAAAAAAAAAAAAAAAAAABgAAAAYAAAYHAAAOAYAADADAABwwwAAYMMAAODDABDAZgAZwDwAHYAAAB4AAAAf+AAAH/AAAB/gAAAfwAAAH4AAAB8AAAAeAAAAHAAAABgAAAAQAAAAAAAAAAAAAAAAAAAA/////////////+f////D////5//////////n////w///58P//8PB//+D4P//hzB//wYYf98OGH/ODhh/xh8A/8Afgf/AP8P/wAH//8AD///AB///wA///8Af///AP///wH///8D////B////w////8f////P////3/////////8=", scale: 1 },
];

function readUint16Le(bytes: Uint8Array, offset: number): number {
  return bytes[offset]! | (bytes[offset + 1]! << 8);
}

function decodeUtf8(bytes: Uint8Array): string {
  return new TextDecoder("utf-8").decode(bytes);
}

function parseOptionalCursorPosition(bytes: Uint8Array, offset: number): {
  position?: GfnCursorPosition;
  nextOffset: number;
} {
  if (offset + 4 > bytes.byteLength) {
    return { nextOffset: offset };
  }
  return {
    position: {
      x: readUint16Le(bytes, offset),
      y: readUint16Le(bytes, offset + 2),
    },
    nextOffset: offset + 4,
  };
}

export function parseGfnCursorChannelMessage(bytes: Uint8Array): GfnCursorChannelMessage | null {
  const messageType = bytes[0];
  if (messageType !== 0 && messageType !== 1) {
    return null;
  }

  const cursorId = bytes[1];
  if (cursorId === undefined) {
    return null;
  }

  if (bytes.byteLength < 5) {
    return messageType === 0 ? { type: "predefined", cursorId } : null;
  }

  const hotspotX = bytes[2]!;
  const hotspotY = bytes[3]!;
  const mimeTypeLength = bytes[4]!;
  let offset = 5;

  if (offset + mimeTypeLength > bytes.byteLength) {
    return null;
  }
  const mimeType = mimeTypeLength > 0
    ? decodeUtf8(bytes.subarray(offset, offset + mimeTypeLength))
    : "";
  offset += mimeTypeLength;

  if (offset + 2 > bytes.byteLength) {
    if (messageType === 0) {
      return { type: "predefined", cursorId };
    }
    return null;
  }
  const imageLength = readUint16Le(bytes, offset);
  offset += 2;

  if (offset + imageLength > bytes.byteLength) {
    return null;
  }
  const imageBase64 = imageLength > 0
    ? decodeUtf8(bytes.subarray(offset, offset + imageLength))
    : "";
  offset += imageLength;

  const positionResult = parseOptionalCursorPosition(bytes, offset);
  offset = positionResult.nextOffset;

  let scale = 1;
  if (offset + 2 <= bytes.byteLength) {
    const rawScale = readUint16Le(bytes, offset) / 100;
    if (Number.isFinite(rawScale) && rawScale > 0) {
      scale = rawScale;
    }
  }

  if (messageType === 0) {
    return {
      type: "predefined",
      cursorId,
      position: positionResult.position,
    };
  }

  return {
    type: "custom",
    cursorId,
    hotspotX,
    hotspotY,
    mimeType,
    imageBase64,
    position: positionResult.position,
    scale,
  };
}

export function cursorDevicePixelRatioScale(value: number): number {
  if (value >= 1.499 && value < 1.999) {
    return 2;
  }
  return Math.max(1, Math.floor(value + 0.001));
}

export function nativeCursorStyle(
  dataUrl: string,
  hotspotX: number,
  hotspotY: number,
  devicePixelRatio: number,
  imageSetFunction: "image-set" | "-webkit-image-set" | null,
): string {
  const x = Math.max(0, hotspotX);
  const y = Math.max(0, hotspotY);
  if (imageSetFunction) {
    return `${imageSetFunction}(url(${dataUrl}) ${devicePixelRatio}x) ${x} ${y}, auto`;
  }
  return `url(${dataUrl}) ${x} ${y}, auto`;
}

export function shouldApplyCursorChannelPosition(
  wasCursorVisible: boolean,
  nextCursorVisible: boolean,
  position?: GfnCursorPosition,
): position is GfnCursorPosition {
  return !wasCursorVisible && nextCursorVisible && position !== undefined;
}

export class GfnCursorOverlayController {
  private readonly canvas: HTMLCanvasElement;
  private readonly context: CanvasRenderingContext2D | null;
  private readonly cursorCache = new Map<number, GfnCursorShape>();
  private readonly imageSetFunction: "image-set" | "-webkit-image-set" | null;
  private readonly originalCursor: string;
  private readonly resizeObserver: ResizeObserver | null = null;
  private readonly onWindowResize = (): void => this.refresh();
  private readonly onVideoResize = (): void => this.refresh();

  private currentCursor: GfnCursorShape = PREDEFINED_CURSORS[1]!;
  private cursorVisible = false;
  private pointerLocked = false;
  private positionX = 0;
  private positionY = 0;
  private positionInitialized = false;
  private fallbackResolution: { width: number; height: number } | null = null;
  private imageLoadGeneration = 0;

  constructor(private readonly videoElement: HTMLVideoElement) {
    for (const cursor of PREDEFINED_CURSORS) {
      this.cursorCache.set(cursor.id, cursor);
    }

    this.canvas = document.createElement("canvas");
    this.context = this.canvas.getContext("2d");
    this.canvas.style.position = "absolute";
    this.canvas.style.zIndex = "200";
    this.canvas.style.left = "0px";
    this.canvas.style.top = "0px";
    this.canvas.style.pointerEvents = "none";
    this.canvas.style.touchAction = "none";
    this.canvas.style.willChange = "transform";
    this.canvas.style.visibility = "hidden";
    this.canvas.style.display = "block";
    this.canvas.style.imageRendering = "pixelated";
    videoElement.insertAdjacentElement("afterend", this.canvas);

    this.imageSetFunction = typeof CSS !== "undefined"
      ? (["image-set", "-webkit-image-set"] as const).find((fn) =>
        CSS.supports("cursor", `${fn}(url(image.bmp) 2x) 0 0, auto`)
      ) ?? null
      : null;
    this.originalCursor = videoElement.style.cursor;

    if (typeof ResizeObserver !== "undefined") {
      this.resizeObserver = new ResizeObserver(() => this.refresh());
      this.resizeObserver.observe(videoElement);
    } else {
      window.addEventListener("resize", this.onWindowResize);
    }
    videoElement.addEventListener("resize", this.onVideoResize);
    this.refresh();
  }

  public dispose(): void {
    this.imageLoadGeneration++;
    this.resizeObserver?.disconnect();
    window.removeEventListener("resize", this.onWindowResize);
    this.videoElement.removeEventListener("resize", this.onVideoResize);
    this.videoElement.style.cursor = this.originalCursor;
    this.canvas.remove();
  }

  public setFallbackResolution(resolution: { width: number; height: number } | null): void {
    this.fallbackResolution = resolution;
    this.refresh();
  }

  public setPointerLocked(active: boolean): void {
    if (this.pointerLocked === active) {
      return;
    }
    this.pointerLocked = active;
    this.applyCursorVisibility();
  }

  public handleMessage(bytes: Uint8Array): boolean {
    const message = parseGfnCursorChannelMessage(bytes);
    if (!message) {
      return false;
    }

    if (message.type === "predefined") {
      const cursor = this.cursorCache.get(message.cursorId) ?? this.cursorCache.get(1)!;
      this.applyCursor(cursor, message.position);
      return true;
    }

    const cached = this.cursorCache.get(message.cursorId);
    if (!message.imageBase64) {
      if (cached) {
        this.applyCursor(cached, message.position);
        return true;
      }
      return false;
    }

    const cursor: GfnCursorShape = {
      id: message.cursorId,
      style: "custom",
      hotspotX: message.hotspotX,
      hotspotY: message.hotspotY,
      mimeType: message.mimeType || "image/png",
      imageBase64: message.imageBase64,
      scale: message.scale,
    };
    this.cursorCache.set(message.cursorId, cursor);
    this.applyCursor(cursor, message.position);
    return true;
  }

  public moveBy(dx: number, dy: number): void {
    if (!this.cursorVisible || !Number.isFinite(dx) || !Number.isFinite(dy)) {
      return;
    }
    const viewport = this.getViewport();
    if (!this.positionInitialized) {
      this.positionX = viewport.width / 2;
      this.positionY = viewport.height / 2;
      this.positionInitialized = true;
    }
    this.positionX = Math.max(0, Math.min(viewport.width, this.positionX + dx));
    this.positionY = Math.max(0, Math.min(viewport.height, this.positionY + dy));
    this.positionCanvas(viewport);
  }

  public setClientPosition(clientX: number, clientY: number): void {
    if (!Number.isFinite(clientX) || !Number.isFinite(clientY)) {
      return;
    }
    const viewport = this.getViewport();
    const parentRect = this.videoElement.parentElement?.getBoundingClientRect() ?? new DOMRect(0, 0, 0, 0);
    this.positionX = Math.max(0, Math.min(viewport.width, clientX - parentRect.left - viewport.originX));
    this.positionY = Math.max(0, Math.min(viewport.height, clientY - parentRect.top - viewport.originY));
    this.positionInitialized = true;
    this.positionCanvas(viewport);
  }

  public refresh(): void {
    const viewport = this.getViewport();
    if (!this.positionInitialized) {
      this.positionX = viewport.width / 2;
      this.positionY = viewport.height / 2;
      this.positionInitialized = true;
    } else {
      this.positionX = Math.max(0, Math.min(viewport.width, this.positionX));
      this.positionY = Math.max(0, Math.min(viewport.height, this.positionY));
    }
    this.rasterizeCurrentCursor();
    this.positionCanvas(viewport);
    this.applyCursorVisibility();
  }

  private applyCursor(cursor: GfnCursorShape, normalizedPosition?: GfnCursorPosition): void {
    const wasCursorVisible = this.cursorVisible;
    this.currentCursor = cursor;
    this.cursorVisible = cursor.style !== "none";
    if (shouldApplyCursorChannelPosition(wasCursorVisible, this.cursorVisible, normalizedPosition)) {
      const viewport = this.getViewport();
      this.positionX = normalizedPosition.x * viewport.width / CURSOR_POSITION_MAX;
      this.positionY = normalizedPosition.y * viewport.height / CURSOR_POSITION_MAX;
      this.positionInitialized = true;
    }

    if (cursor.imageBase64 && !cursor.image) {
      const generation = ++this.imageLoadGeneration;
      const image = new Image();
      cursor.image = image;
      image.decoding = "async";
      image.src = `data:${cursor.mimeType};base64,${cursor.imageBase64}`;
      if (image.decode) {
        void image.decode()
          .then(() => {
            if (generation === this.imageLoadGeneration && this.currentCursor === cursor) {
              this.refresh();
            }
          })
          .catch(() => {
            if (generation === this.imageLoadGeneration && this.currentCursor === cursor) {
              this.cursorVisible = false;
              this.applyCursorVisibility();
            }
          });
      } else {
        image.onload = () => {
          if (generation === this.imageLoadGeneration && this.currentCursor === cursor) {
            this.refresh();
          }
        };
        image.onerror = () => {
          if (generation === this.imageLoadGeneration && this.currentCursor === cursor) {
            this.cursorVisible = false;
            this.applyCursorVisibility();
          }
        };
      }
    }

    this.refresh();
  }

  private getViewport(): StreamViewport {
    const rect = this.videoElement.getBoundingClientRect();
    const parentRect = this.videoElement.parentElement?.getBoundingClientRect() ?? new DOMRect(0, 0, 0, 0);
    const clientWidth = rect.width || this.videoElement.clientWidth || this.fallbackResolution?.width || 1;
    const clientHeight = rect.height || this.videoElement.clientHeight || this.fallbackResolution?.height || 1;
    const sourceWidth = this.videoElement.videoWidth || this.fallbackResolution?.width || clientWidth;
    const sourceHeight = this.videoElement.videoHeight || this.fallbackResolution?.height || clientHeight;
    const safeSourceWidth = sourceWidth > 0 ? sourceWidth : clientWidth;
    const safeSourceHeight = sourceHeight > 0 ? sourceHeight : clientHeight;

    let width = clientWidth;
    let height = clientHeight;
    let offsetX = 0;
    let offsetY = 0;
    if (clientWidth / clientHeight > safeSourceWidth / safeSourceHeight) {
      const scale = clientHeight / safeSourceHeight;
      width = safeSourceWidth * scale;
      height = clientHeight;
      offsetX = (clientWidth - width) / 2;
    } else {
      const scale = clientWidth / safeSourceWidth;
      width = clientWidth;
      height = safeSourceHeight * scale;
      offsetY = (clientHeight - height) / 2;
    }

    return {
      originX: rect.left - parentRect.left + offsetX,
      originY: rect.top - parentRect.top + offsetY,
      width: Math.max(1, width),
      height: Math.max(1, height),
    };
  }

  private rasterizeCurrentCursor(): void {
    const cursor = this.currentCursor;
    if (!cursor.image || !this.context || cursor.image.naturalWidth <= 0 || cursor.image.naturalHeight <= 0) {
      this.canvas.width = 0;
      this.canvas.height = 0;
      cursor.nativeStyle = cursor.style === "custom" ? "default" : cursor.style;
      return;
    }

    const devicePixelRatio = window.devicePixelRatio || 1;
    const renderScale = cursorDevicePixelRatioScale(devicePixelRatio / cursor.scale);
    const cssScale = renderScale / devicePixelRatio;
    const bitmapWidth = Math.ceil(cursor.image.width * renderScale);
    const bitmapHeight = Math.ceil(cursor.image.height * renderScale);
    if (this.canvas.width !== bitmapWidth || this.canvas.height !== bitmapHeight) {
      this.canvas.width = bitmapWidth;
      this.canvas.height = bitmapHeight;
      this.canvas.style.width = `${cursor.image.width * cssScale}px`;
      this.canvas.style.height = `${cursor.image.height * cssScale}px`;
    } else {
      this.context.clearRect(0, 0, this.canvas.width, this.canvas.height);
    }

    this.context.imageSmoothingEnabled = false;
    this.context.drawImage(cursor.image, 0, 0, bitmapWidth, bitmapHeight);
    const dataUrl = this.canvas.toDataURL("image/png");
    cursor.nativeStyle = nativeCursorStyle(
      dataUrl,
      cursor.hotspotX * cssScale,
      cursor.hotspotY * cssScale,
      devicePixelRatio,
      this.imageSetFunction,
    );
  }

  private positionCanvas(viewport: StreamViewport = this.getViewport()): void {
    const cursor = this.currentCursor;
    const devicePixelRatio = window.devicePixelRatio || 1;
    const renderScale = cursorDevicePixelRatioScale(devicePixelRatio / cursor.scale);
    const cssScale = renderScale / devicePixelRatio;
    const x = viewport.originX + this.positionX - cursor.hotspotX * cssScale;
    const y = viewport.originY + this.positionY - cursor.hotspotY * cssScale;
    this.canvas.style.left = `${viewport.originX}px`;
    this.canvas.style.top = `${viewport.originY}px`;
    this.canvas.style.transform = `translate(${Math.round(x - viewport.originX)}px, ${Math.round(y - viewport.originY)}px)`;
  }

  private applyCursorVisibility(): void {
    const hasOverlayImage =
      this.cursorVisible
      && !!this.currentCursor.image
      && this.currentCursor.image.naturalWidth > 0
      && this.currentCursor.image.naturalHeight > 0;
    if (this.pointerLocked) {
      this.videoElement.style.cursor = "none";
      this.canvas.style.visibility = hasOverlayImage ? "visible" : "hidden";
      return;
    }

    this.canvas.style.visibility = "hidden";
    if (!this.cursorVisible) {
      this.videoElement.style.cursor = "none";
      return;
    }
    this.videoElement.style.cursor = this.currentCursor.nativeStyle || this.currentCursor.style || "default";
  }
}
