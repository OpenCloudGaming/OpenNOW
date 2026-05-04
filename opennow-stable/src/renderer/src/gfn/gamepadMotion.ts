export interface GamepadMotionSample {
  receivedAtMs: number;
  gyroX: number;
  gyroY: number;
  gyroZ: number;
  accelX: number;
  accelY: number;
  accelZ: number;
  sensorTimestamp: number;
}

type HidDeviceFilter = { vendorId?: number; productId?: number };
type HidRequestOptions = { filters: HidDeviceFilter[] };
type HidInputReportEvent = Event & { device: HidDevice; reportId: number; data: DataView };
type HidDevice = EventTarget & {
  vendorId: number;
  productId: number;
  productName?: string;
  opened: boolean;
  open(): Promise<void>;
  close(): Promise<void>;
  addEventListener(type: "inputreport", listener: (event: HidInputReportEvent) => void): void;
  removeEventListener(type: "inputreport", listener: (event: HidInputReportEvent) => void): void;
};
type HidNavigator = Navigator & {
  hid?: {
    getDevices(): Promise<HidDevice[]>;
    requestDevice(options: HidRequestOptions): Promise<HidDevice[]>;
  };
};

const SONY_VENDOR_ID = 0x054c;
const DEFAULT_FRESHNESS_MS = 150;
const DS4_PRODUCT_IDS = new Set([0x05c4, 0x09cc]);
const DUALSENSE_PRODUCT_IDS = new Set([0x0ce6, 0x0df2]);

export function isSonyGamepad(gamepad: Gamepad): boolean {
  return /054c|sony|wireless controller|dualsense|dualshock/i.test(gamepad.id);
}

export async function requestSonyGamepadHidAccess(log: (line: string) => void = () => {}): Promise<boolean> {
  const hid = (navigator as HidNavigator).hid;
  if (!hid?.requestDevice) {
    log("Experimental gamepad gyro permission skipped: WebHID is not exposed");
    return false;
  }

  try {
    const devices = await hid.requestDevice({ filters: [{ vendorId: SONY_VENDOR_ID }] });
    return devices.some(isSonyHidDevice);
  } catch (error) {
    log(`Experimental gamepad gyro permission request failed: ${error instanceof Error ? error.message : String(error)}`);
    return false;
  }
}

export class GamepadMotionManager {
  private enabled = false;
  private devices: HidDevice[] = [];
  private latestSample: GamepadMotionSample | null = null;
  private requested = false;
  private readonly onReport = (event: HidInputReportEvent): void => this.handleInputReport(event);

  constructor(private readonly log: (line: string) => void = () => {}) {}

  get supported(): boolean {
    return Boolean((navigator as HidNavigator).hid);
  }

  async setEnabled(enabled: boolean, requestPermission = false): Promise<void> {
    if (enabled === this.enabled && (!enabled || !requestPermission || this.requested)) {
      return;
    }
    this.enabled = enabled;
    if (!enabled) {
      this.stop();
      return;
    }
    await this.start(requestPermission);
  }

  async start(requestPermission = false): Promise<void> {
    const hid = (navigator as HidNavigator).hid;
    if (!hid) {
      this.log("Experimental gamepad gyro unavailable: WebHID is not exposed");
      return;
    }

    try {
      const devices = hid.getDevices ? await hid.getDevices() : [];
      await this.openDevices(devices.filter(isSonyHidDevice));
      if (requestPermission && !this.requested) {
        this.requested = true;
        await requestSonyGamepadHidAccess(this.log);
        const requestedDevices = hid.getDevices ? await hid.getDevices() : [];
        await this.openDevices(requestedDevices.filter(isSonyHidDevice));
      }
    } catch (error) {
      this.log(`Experimental gamepad gyro WebHID start failed: ${error instanceof Error ? error.message : String(error)}`);
    }
  }

  stop(): void {
    for (const device of this.devices) {
      try {
        device.removeEventListener("inputreport", this.onReport);
      } catch {}
      try {
        if (device.opened) {
          void device.close().catch(() => {});
        }
      } catch {}
    }
    this.devices = [];
    this.latestSample = null;
  }

  getFreshSample(gamepad: Gamepad, controllerId: number, maxAgeMs = DEFAULT_FRESHNESS_MS): GamepadMotionSample | null {
    if (!this.enabled || controllerId !== 0 || !isSonyGamepad(gamepad)) {
      return null;
    }
    const sample = this.latestSample;
    if (!sample || performance.now() - sample.receivedAtMs > maxAgeMs) {
      return null;
    }
    return sample;
  }

  private async openDevices(devices: HidDevice[]): Promise<void> {
    for (const device of devices) {
      if (this.devices.includes(device)) {
        continue;
      }
      try {
        if (!device.opened) {
          await device.open();
        }
        device.addEventListener("inputreport", this.onReport);
        this.devices.push(device);
        this.log(`Experimental gamepad gyro opened Sony HID device: ${device.productName || `0x${device.productId.toString(16)}`}`);
      } catch (error) {
        this.log(`Experimental gamepad gyro could not open Sony HID device: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  }

  private handleInputReport(event: HidInputReportEvent): void {
    if (!isSonyHidDevice(event.device)) {
      return;
    }
    const sample = parseSonyMotionReport(event.device, event.reportId, event.data);
    if (sample) {
      this.latestSample = sample;
    }
  }
}

function isSonyHidDevice(device: HidDevice): boolean {
  return device.vendorId === SONY_VENDOR_ID;
}

function parseSonyMotionReport(device: HidDevice, reportId: number, data: DataView): GamepadMotionSample | null {
  if (reportId !== 1) {
    return null;
  }

  if (isDualSenseHidDevice(device)) {
    return data.byteLength >= 27 ? sampleFromOffsets(data, 15, 17, 19, 21, 23, 25) : null;
  }

  if (isDs4HidDevice(device) || !isDualSenseHidDevice(device)) {
    return data.byteLength >= 24 ? sampleFromOffsets(data, 12, 14, 16, 18, 20, 22) : null;
  }

  return null;
}

function isDs4HidDevice(device: HidDevice): boolean {
  const name = device.productName ?? "";
  return DS4_PRODUCT_IDS.has(device.productId) || /dualshock|wireless controller/i.test(name);
}

function isDualSenseHidDevice(device: HidDevice): boolean {
  const name = device.productName ?? "";
  return DUALSENSE_PRODUCT_IDS.has(device.productId) || /dualsense/i.test(name);
}

function sampleFromOffsets(data: DataView, gx: number, gy: number, gz: number, ax: number, ay: number, az: number): GamepadMotionSample {
  const now = performance.now();
  return {
    receivedAtMs: now,
    gyroX: data.getInt16(gx, true),
    gyroY: data.getInt16(gy, true),
    gyroZ: data.getInt16(gz, true),
    accelX: data.getInt16(ax, true),
    accelY: data.getInt16(ay, true),
    accelZ: data.getInt16(az, true),
    sensorTimestamp: Math.floor(now) & 0xffff,
  };
}
