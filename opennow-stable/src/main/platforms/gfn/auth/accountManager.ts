import type { AuthSession, AuthSessionResult, SavedAccount } from "@shared/gfn";

import type { ConsoleProfileStore } from "./consoleProfileStore";
import type { PersistedAccountState } from "./persistedAccountState";

type EnsureSession = (forceRefresh: boolean, expectedUserId?: string) => Promise<AuthSessionResult>;

export class AccountManager {
  constructor(
    private readonly state: PersistedAccountState,
    private readonly clearCaches: () => void,
    /** Optional so callers that never touch console PINs stay unchanged. */
    private readonly consoleProfiles?: ConsoleProfileStore,
  ) {}

  setSession(session: AuthSession | null): void {
    if (!session) {
      this.state.accounts.reset();
      this.clearCaches();
      void this.state.persist();
      return;
    }

    this.state.accounts.setSession(session);
    this.clearCaches();
    void this.state.persist();
  }

  getSavedAccounts(): SavedAccount[] {
    // The account state has no view of the lock store, so the PIN flag is
    // decorated here. Only the boolean crosses the process boundary.
    return this.state.accounts.getSavedAccounts().map((account) => ({
      ...account,
      hasPin: this.consoleProfiles?.hasPin(account.userId) ?? false,
    }));
  }

  async saveLoginSession(
    session: AuthSession,
    enrichUserTier: () => Promise<void>,
  ): Promise<AuthSession> {
    this.state.accounts.setSession(session);
    this.clearCaches();
    await enrichUserTier();
    await this.state.persist();
    return this.state.accounts.getSession() as AuthSession;
  }

  async switchAccount(userId: string, ensureSession: EnsureSession): Promise<AuthSession> {
    const target = this.state.accounts.getSessionForUser(userId);
    if (!target) {
      throw new Error("Saved account not found");
    }

    const previousActiveUserId = this.state.accounts.getActiveUserId();
    const previousSelectedProvider = this.state.accounts.getPersistedSelectedProvider();

    this.state.accounts.setActiveAccount(userId);
    this.clearCaches();

    const result = await ensureSession(true, userId);
    const missingRefreshToken = result.refresh.outcome === "missing_refresh_token";
    const refreshFailed = result.refresh.outcome === "failed";
    const switchedUserMismatch = result.session?.user.userId !== userId;
    if (!result.session || refreshFailed || missingRefreshToken || switchedUserMismatch) {
      const fallbackMessage = "Failed to switch account due to an invalid or expired session.";

      if (missingRefreshToken) {
        await this.removeAccount(userId);
        this.state.accounts.setActiveAccount(previousActiveUserId);
        this.clearCaches();
        await this.state.persist();
        throw new Error("Saved login for this account is incomplete. Please log in to this account again.");
      }

      this.state.accounts.setActiveAccount(previousActiveUserId);
      if (previousActiveUserId && this.state.accounts.hasAccount(previousActiveUserId)) {
        this.state.accounts.setSelectedProvider(previousSelectedProvider);
      }
      this.clearCaches();
      await this.state.persist();

      if (switchedUserMismatch) {
        throw new Error("Switched session did not match the selected account.");
      }
      throw new Error(result.refresh.message || fallbackMessage);
    }
    return result.session;
  }

  async removeAccount(userId: string): Promise<void> {
    const removed = this.state.accounts.removeAccount(userId);
    if (!removed) {
      return;
    }
    if (this.state.accounts.getActiveUserId() === userId) {
      this.state.accounts.setActiveAccount(this.state.accounts.firstUserId());
    }
    this.clearCaches();
    await this.state.persist();
    await this.consoleProfiles?.forgetUser(userId);
  }

  async logout(): Promise<void> {
    const activeUserId = this.state.accounts.getActiveUserId();
    if (!activeUserId) {
      return;
    }
    this.state.accounts.removeAccount(activeUserId);
    this.state.accounts.setActiveAccount(this.state.accounts.firstUserId());
    this.clearCaches();
    await this.state.persist();
    await this.consoleProfiles?.forgetUser(activeUserId);
  }

  async logoutAll(): Promise<void> {
    this.state.accounts.reset();
    this.clearCaches();
    await this.state.persist();
    await this.consoleProfiles?.forgetAll();
  }
}
