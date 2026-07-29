import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

import type { AuthSession, LoginProvider, SavedAccountIdentity } from "@shared/gfn";

import { defaultProvider, normalizeProvider } from "./providerDiscovery";

export interface PersistedAuthState {
  sessions: AuthSession[];
  activeUserId: string | null;
  selectedProvider: LoginProvider | null;
}

export type RestorableAuthState = Partial<PersistedAuthState> & {
  session?: AuthSession | null;
};

export class AccountState {
  private sessions = new Map<string, AuthSession>();
  private activeUserId: string | null = null;
  private selectedProvider: LoginProvider = defaultProvider();

  restore(parsed: RestorableAuthState): void {
    if (parsed.selectedProvider) {
      this.selectedProvider = normalizeProvider(parsed.selectedProvider);
    }

    this.sessions.clear();
    if (Array.isArray(parsed.sessions)) {
      for (const persistedSession of parsed.sessions) {
        if (!persistedSession?.user?.userId) {
          continue;
        }
        this.sessions.set(persistedSession.user.userId, {
          ...persistedSession,
          provider: normalizeProvider(persistedSession.provider),
        });
      }
    } else if (parsed.session?.user?.userId) {
      this.sessions.set(parsed.session.user.userId, {
        ...parsed.session,
        provider: normalizeProvider(parsed.session.provider),
      });
    }

    this.activeUserId =
      typeof parsed.activeUserId === "string" && this.sessions.has(parsed.activeUserId)
        ? parsed.activeUserId
        : this.sessions.keys().next().value ?? null;
  }

  snapshot(): PersistedAuthState {
    return {
      sessions: Array.from(this.sessions.values()),
      activeUserId: this.activeUserId,
      selectedProvider: this.selectedProvider,
    };
  }

  reset(): void {
    this.sessions.clear();
    this.activeUserId = null;
    this.selectedProvider = defaultProvider();
  }

  getSession(): AuthSession | null {
    if (!this.activeUserId) {
      return null;
    }
    return this.sessions.get(this.activeUserId) ?? null;
  }

  getSessionForUser(userId: string): AuthSession | null {
    return this.sessions.get(userId) ?? null;
  }

  hasAccount(userId: string): boolean {
    return this.sessions.has(userId);
  }

  firstUserId(): string | null {
    return this.sessions.keys().next().value ?? null;
  }

  getActiveUserId(): string | null {
    return this.activeUserId;
  }

  getSelectedProvider(): LoginProvider {
    return this.getSession()?.provider ?? this.selectedProvider;
  }

  getPersistedSelectedProvider(): LoginProvider {
    return this.selectedProvider;
  }

  setSelectedProvider(provider: LoginProvider): void {
    this.selectedProvider = normalizeProvider(provider);
  }

  setSession(session: AuthSession): AuthSession {
    const normalized: AuthSession = {
      ...session,
      provider: normalizeProvider(session.provider),
    };
    this.sessions.set(normalized.user.userId, normalized);
    this.activeUserId = normalized.user.userId;
    this.selectedProvider = normalized.provider;
    return normalized;
  }

  updateSession(session: AuthSession): void {
    this.sessions.set(session.user.userId, session);
  }

  setActiveAccount(userId: string | null): void {
    this.activeUserId = userId && this.sessions.has(userId) ? userId : null;
    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
  }

  removeAccount(userId: string): boolean {
    return this.sessions.delete(userId);
  }

  /** Identity only — the console PIN flag is decorated by AccountManager. */
  getSavedAccounts(): SavedAccountIdentity[] {
    return Array.from(this.sessions.values()).map((session) => ({
      userId: session.user.userId,
      displayName: session.user.displayName,
      email: session.user.email,
      avatarUrl: session.user.avatarUrl,
      membershipTier: session.user.membershipTier,
      providerCode: session.provider.code,
    }));
  }
}

export class PersistedAccountState {
  readonly accounts = new AccountState();

  constructor(private readonly statePath: string) {}

  async initialize(): Promise<AuthSession | null> {
    try {
      await access(this.statePath);
    } catch {
      await this.persist();
      return null;
    }

    try {
      const raw = await readFile(this.statePath, "utf8");
      this.accounts.restore(JSON.parse(raw) as RestorableAuthState);
      return this.accounts.getSession();
    } catch {
      this.accounts.reset();
      await this.persist();
      return null;
    }
  }

  async persist(): Promise<void> {
    await mkdir(dirname(this.statePath), { recursive: true });
    await writeFile(
      this.statePath,
      JSON.stringify(this.accounts.snapshot(), null, 2),
      "utf8",
    );
  }
}
