export interface AdaptiveMouseFlushDecisionParams {
  baseIntervalMs: number;
  currentIntervalMs: number;
  reliableBufferedAmount: number;
  schedulingDelayMs: number;
  canUsePartiallyReliableMouse: boolean;
  backpressureThresholdBytes: number;
  minIntervalMs: number;
  maxIntervalMs: number;
}

export function chooseAdaptiveMouseFlushInterval(params: AdaptiveMouseFlushDecisionParams): number {
  const boundedBase = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.baseIntervalMs));
  const boundedCurrent = Math.max(params.minIntervalMs, Math.min(params.maxIntervalMs, params.currentIntervalMs));
  if (!params.canUsePartiallyReliableMouse) {
    return boundedBase;
  }

  const highPressure =
    params.reliableBufferedAmount >= params.backpressureThresholdBytes / 2 || params.schedulingDelayMs >= 4;
  if (highPressure) {
    return Math.max(boundedBase, Math.min(params.maxIntervalMs, boundedCurrent + 2));
  }

  const lowPressure = params.reliableBufferedAmount <= 4096 && params.schedulingDelayMs <= 1;
  if (lowPressure) {
    return Math.max(params.minIntervalMs, boundedCurrent - 1);
  }

  if (boundedCurrent > boundedBase) {
    return Math.max(boundedBase, boundedCurrent - 1);
  }
  if (boundedCurrent < boundedBase) {
    return Math.min(boundedBase, boundedCurrent + 1);
  }
  return boundedCurrent;
}

export function quantizeMouseDeltaWithResidual(accumulatedDelta: number): { send: number; residual: number } {
  const send = Math.round(accumulatedDelta);
  return {
    send,
    residual: accumulatedDelta - send,
  };
}

export class MouseDeltaFilter {
  private x = 0;
  private y = 0;
  private lastTsMs = 0;
  private velocityX = 0;
  private velocityY = 0;
  private rejectedX = 0;
  private rejectedY = 0;
  private pendingX = 0;
  private pendingY = 0;
  private sawZero = false;
  private relaxedForRawInput = false;

  public setRelaxedForRawInput(value: boolean): void {
    this.relaxedForRawInput = value;
  }

  public getX(): number {
    return this.x;
  }

  public getY(): number {
    return this.y;
  }

  public reset(): void {
    this.x = 0;
    this.y = 0;
    this.lastTsMs = 0;
    this.velocityX = 0;
    this.velocityY = 0;
    this.rejectedX = 0;
    this.rejectedY = 0;
    this.pendingX = 0;
    this.pendingY = 0;
    this.sawZero = false;
  }

  public update(dx: number, dy: number, tsMs: number): boolean {
    if (dx === 0 && dy === 0) {
      if (this.sawZero) {
        this.pendingX = 0;
        this.pendingY = 0;
      } else {
        this.sawZero = true;
      }
      return false;
    }

    this.sawZero = false;
    if (this.pendingX === 0 && this.pendingY === 0) {
      if (tsMs < this.lastTsMs) {
        this.pendingX = dx;
        this.pendingY = dy;
        return false;
      }
    } else {
      dx += this.pendingX;
      dy += this.pendingY;
      this.pendingX = 0;
      this.pendingY = 0;
    }

    const dot = dx * this.x + dy * this.y;
    const magIncoming = dx * dx + dy * dy;
    const magPrev = this.x * this.x + this.y * this.y;
    let accept = true;

    const dtMs = tsMs - this.lastTsMs;
    const directionReversalCosineThreshold = this.relaxedForRawInput ? 0.89 : 0.81;
    if (dtMs < 0.95 && dot < 0 && magPrev !== 0 && dot * dot > directionReversalCosineThreshold * magIncoming * magPrev) {
      const ratio = Math.sqrt(magIncoming) / Math.sqrt(magPrev);
      let distToInt = Math.abs(ratio - Math.trunc(ratio));
      if (distToInt > 0.5) {
        distToInt = 1 - distToInt;
      }
      const intRatioRejectThreshold = this.relaxedForRawInput ? 0.07 : 0.1;
      if (distToInt < intRatioRejectThreshold) {
        accept = false;
      }
    }

    const diffX = dx - this.x;
    const diffY = dy - this.y;
    const diffMag = diffX * diffX + diffY * diffY;

    if (accept) {
      const scale = 1 + 0.1 * Math.max(1, Math.min(16, dtMs));
      const vx2 = 2 * scale * Math.abs(this.velocityX);
      const vy2 = 2 * scale * Math.abs(this.velocityY);
      const threshold = Math.max(this.relaxedForRawInput ? 9800 : 8100, vx2 * vx2 + vy2 * vy2);
      accept = diffMag < threshold;
      if (!accept && (this.rejectedX !== 0 || this.rejectedY !== 0)) {
        const rx = dx - this.rejectedX;
        const ry = dy - this.rejectedY;
        accept = rx * rx + ry * ry < threshold;
      }
    }

    if (accept) {
      this.velocityX = 0.4 * this.velocityX + 0.6 * diffX;
      this.velocityY = 0.4 * this.velocityY + 0.6 * diffY;
      this.x = dx;
      this.y = dy;
      this.lastTsMs = tsMs;
      this.rejectedX = 0;
      this.rejectedY = 0;
      return true;
    }

    this.rejectedX = dx;
    this.rejectedY = dy;
    return false;
  }
}
