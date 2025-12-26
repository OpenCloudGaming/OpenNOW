// Keyboard Lock API (Experimental)
// https://developer.mozilla.org/en-US/docs/Web/API/Keyboard/lock

interface Keyboard {
  lock(keyCodes?: string[]): Promise<void>;
  unlock(): void;
}

interface Navigator {
  readonly keyboard?: Keyboard;
}
