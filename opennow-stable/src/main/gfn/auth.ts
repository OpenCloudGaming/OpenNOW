     1	import { createServer } from "node:http";
     2	import { createHash, randomBytes } from "node:crypto";
     3	import { access, mkdir, readFile, writeFile } from "node:fs/promises";
     4	import { dirname } from "node:path";
     5	import net from "node:net";
     6	import os from "node:os";
     7	
     8	import { shell } from "electron";
     9	
    10	import type {
    11	  AuthLoginRequest,
    12	  AuthSession,
    13	  AuthSessionResult,
    14	  AuthTokens,
    15	  AuthUser,
    16	  LoginProvider,
    17	  SavedAccount,
    18	  StreamRegion,
    19	  SubscriptionInfo,
    20	} from "@shared/gfn";
    21	import { fetchSubscription, fetchDynamicRegions } from "./subscription";
    22	
    23	const SERVICE_URLS_ENDPOINT = "https://pcs.geforcenow.com/v1/serviceUrls";
    24	const TOKEN_ENDPOINT = "https://login.nvidia.com/token";
    25	const CLIENT_TOKEN_ENDPOINT = "https://login.nvidia.com/client_token";
    26	const USERINFO_ENDPOINT = "https://login.nvidia.com/userinfo";
    27	const AUTH_ENDPOINT = "https://login.nvidia.com/authorize";
    28	
    29	const CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
    30	const SCOPES = "openid consent email tk_client age";
    31	const DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
    32	
    33	const GFN_USER_AGENT =
    34	  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
    35	
    36	const REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096];
    37	const TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1000;
    38	const CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1000;
    39	
    40	interface PersistedAuthState {
    41	  sessions: AuthSession[];
    42	  activeUserId: string | null;
    43	  selectedProvider: LoginProvider | null;
    44	}
    45	
    46	interface ServiceUrlsResponse {
    47	  requestStatus?: {
    48	    statusCode?: number;
    49	  };
    50	  gfnServiceInfo?: {
    51	    gfnServiceEndpoints?: Array<{
    52	      idpId: string;
    53	      loginProviderCode: string;
    54	      loginProviderDisplayName: string;
    55	      streamingServiceUrl: string;
    56	      loginProviderPriority?: number;
    57	    }>;
    58	  };
    59	}
    60	
    61	interface TokenResponse {
    62	  access_token: string;
    63	  refresh_token?: string;
    64	  id_token?: string;
    65	  client_token?: string;
    66	  expires_in?: number;
    67	}
    68	
    69	interface ClientTokenResponse {
    70	  client_token: string;
    71	  expires_in?: number;
    72	}
    73	
    74	interface ServerInfoResponse {
    75	  requestStatus?: {
    76	    serverId?: string;
    77	  };
    78	  metaData?: Array<{
    79	    key: string;
    80	    value: string;
    81	  }>;
    82	}
    83	
    84	function defaultProvider(): LoginProvider {
    85	  return {
    86	    idpId: DEFAULT_IDP_ID,
    87	    code: "NVIDIA",
    88	    displayName: "NVIDIA",
    89	    streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
    90	    priority: 0,
    91	  };
    92	}
    93	
    94	function normalizeProvider(provider: LoginProvider): LoginProvider {
    95	  return {
    96	    ...provider,
    97	    streamingServiceUrl: provider.streamingServiceUrl.endsWith("/")
    98	      ? provider.streamingServiceUrl
    99	      : `${provider.streamingServiceUrl}/`,
   100	  };
   101	}
   102	
   103	function decodeBase64Url(value: string): string {
   104	  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
   105	  const padding = normalized.length % 4;
   106	  const padded = padding === 0 ? normalized : `${normalized}${"=".repeat(4 - padding)}`;
   107	  return Buffer.from(padded, "base64").toString("utf8");
   108	}
   109	
   110	function parseJwtPayload<T>(token: string): T | null {
   111	  const parts = token.split(".");
   112	  if (parts.length !== 3) {
   113	    return null;
   114	  }
   115	  try {
   116	    const payload = decodeBase64Url(parts[1]);
   117	    return JSON.parse(payload) as T;
   118	  } catch {
   119	    return null;
   120	  }
   121	}
   122	
   123	function toExpiresAt(expiresInSeconds: number | undefined, defaultSeconds = 86400): number {
   124	  return Date.now() + (expiresInSeconds ?? defaultSeconds) * 1000;
   125	}
   126	
   127	function isExpired(expiresAt: number | undefined): boolean {
   128	  if (!expiresAt) {
   129	    return true;
   130	  }
   131	  return expiresAt <= Date.now();
   132	}
   133	
   134	function isNearExpiry(expiresAt: number | undefined, windowMs: number): boolean {
   135	  if (!expiresAt) {
   136	    return true;
   137	  }
   138	  return expiresAt - Date.now() < windowMs;
   139	}
   140	
   141	function generateDeviceId(): string {
   142	  const host = os.hostname();
   143	  const username = os.userInfo().username;
   144	  return createHash("sha256").update(`${host}:${username}:opennow-stable`).digest("hex");
   145	}
   146	
   147	function generatePkce(): { verifier: string; challenge: string } {
   148	  const verifier = randomBytes(64)
   149	    .toString("base64")
   150	    .replace(/\+/g, "-")
   151	    .replace(/\//g, "_")
   152	    .replace(/=+$/g, "")
   153	    .slice(0, 86);
   154	
   155	  const challenge = createHash("sha256")
   156	    .update(verifier)
   157	    .digest("base64")
   158	    .replace(/\+/g, "-")
   159	    .replace(/\//g, "_")
   160	    .replace(/=+$/g, "");
   161	
   162	  return { verifier, challenge };
   163	}
   164	
   165	function buildAuthUrl(provider: LoginProvider, challenge: string, port: number): string {
   166	  const redirectUri = `http://localhost:${port}`;
   167	  const nonce = randomBytes(16).toString("hex");
   168	  const params = new URLSearchParams({
   169	    response_type: "code",
   170	    device_id: generateDeviceId(),
   171	    scope: SCOPES,
   172	    client_id: CLIENT_ID,
   173	    redirect_uri: redirectUri,
   174	    ui_locales: "en_US",
   175	    nonce,
   176	    prompt: "select_account",
   177	    code_challenge: challenge,
   178	    code_challenge_method: "S256",
   179	    idp_id: provider.idpId,
   180	  });
   181	  return `${AUTH_ENDPOINT}?${params.toString()}`;
   182	}
   183	
   184	async function isPortAvailable(port: number): Promise<boolean> {
   185	  return new Promise((resolve) => {
   186	    const server = net.createServer();
   187	    server.once("error", () => resolve(false));
   188	    server.once("listening", () => {
   189	      server.close(() => resolve(true));
   190	    });
   191	    server.listen(port, "127.0.0.1");
   192	  });
   193	}
   194	
   195	async function findAvailablePort(): Promise<number> {
   196	  for (const port of REDIRECT_PORTS) {
   197	    if (await isPortAvailable(port)) {
   198	      return port;
   199	    }
   200	  }
   201	
   202	  throw new Error("No available OAuth callback ports");
   203	}
   204	
   205	async function waitForAuthorizationCode(port: number, timeoutMs: number): Promise<string> {
   206	  return new Promise((resolve, reject) => {
   207	    const server = createServer((request, response) => {
   208	      const url = new URL(request.url ?? "/", `http://localhost:${port}`);
   209	      const code = url.searchParams.get("code");
   210	      const error = url.searchParams.get("error");
   211	
   212	      const html = `<!doctype html><html><head><meta charset="utf-8"><title>OpenNOW Login</title></head><body style="font-family:Segoe UI,Arial,sans-serif;background:#0b1220;color:#dbe7ff;display:flex;justify-content:center;align-items:center;height:100vh"><div style="background:#111a2c;padding:24px 28px;border:1px solid #30425f;border-radius:12px;max-width:460px"><h2 style="margin-top:0">OpenNOW Login</h2><p>${
   213	        code
   214	          ? "Login complete. You can close this window and return to OpenNOW Stable."
   215	          : "Login failed or was cancelled. You can close this window and return to OpenNOW Stable."
   216	      }</p></div></body></html>`;
   217	
   218	      response.statusCode = 200;
   219	      response.setHeader("Content-Type", "text/html; charset=utf-8");
   220	      response.end(html);
   221	
   222	      server.close(() => {
   223	        if (code) {
   224	          resolve(code);
   225	          return;
   226	        }
   227	        reject(new Error(error ?? "Authorization failed"));
   228	      });
   229	    });
   230	
   231	    server.listen(port, "127.0.0.1", () => {
   232	      const timer = setTimeout(() => {
   233	        server.close(() => reject(new Error("Timed out waiting for OAuth callback")));
   234	      }, timeoutMs);
   235	
   236	      server.once("close", () => clearTimeout(timer));
   237	    });
   238	  });
   239	}
   240	
   241	async function exchangeAuthorizationCode(code: string, verifier: string, port: number): Promise<AuthTokens> {
   242	  const body = new URLSearchParams({
   243	    grant_type: "authorization_code",
   244	    code,
   245	    redirect_uri: `http://localhost:${port}`,
   246	    code_verifier: verifier,
   247	  });
   248	
   249	  const response = await fetch(TOKEN_ENDPOINT, {
   250	    method: "POST",
   251	    headers: {
   252	      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
   253	      Origin: "https://nvfile",
   254	      Referer: "https://nvfile/",
   255	      Accept: "application/json, text/plain, */*",
   256	      "User-Agent": GFN_USER_AGENT,
   257	    },
   258	    body,
   259	  });
   260	
   261	  if (!response.ok) {
   262	    const text = await response.text();
   263	    throw new Error(`Token exchange failed (${response.status}): ${text.slice(0, 400)}`);
   264	  }
   265	
   266	  const payload = (await response.json()) as TokenResponse;
   267	  return {
   268	    accessToken: payload.access_token,
   269	    refreshToken: payload.refresh_token,
   270	    idToken: payload.id_token,
   271	    expiresAt: toExpiresAt(payload.expires_in),
   272	  };
   273	}
   274	
   275	async function refreshAuthTokens(refreshToken: string): Promise<AuthTokens> {
   276	  const body = new URLSearchParams({
   277	    grant_type: "refresh_token",
   278	    refresh_token: refreshToken,
   279	    client_id: CLIENT_ID,
   280	  });
   281	
   282	  const response = await fetch(TOKEN_ENDPOINT, {
   283	    method: "POST",
   284	    headers: {
   285	      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
   286	      Origin: "https://nvfile",
   287	      Accept: "application/json, text/plain, */*",
   288	      "User-Agent": GFN_USER_AGENT,
   289	    },
   290	    body,
   291	  });
   292	
   293	  if (!response.ok) {
   294	    const text = await response.text();
   295	    throw new Error(`Token refresh failed (${response.status}): ${text.slice(0, 400)}`);
   296	  }
   297	
   298	  const payload = (await response.json()) as TokenResponse;
   299	  return {
   300	    accessToken: payload.access_token,
   301	    refreshToken: payload.refresh_token ?? refreshToken,
   302	    idToken: payload.id_token,
   303	    expiresAt: toExpiresAt(payload.expires_in),
   304	  };
   305	}
   306	
   307	async function requestClientToken(accessToken: string): Promise<{
   308	  token: string;
   309	  expiresAt: number;
   310	  lifetimeMs: number;
   311	}> {
   312	  const response = await fetch(CLIENT_TOKEN_ENDPOINT, {
   313	    headers: {
   314	      Authorization: `Bearer ${accessToken}`,
   315	      Origin: "https://nvfile",
   316	      Accept: "application/json, text/plain, */*",
   317	      "User-Agent": GFN_USER_AGENT,
   318	    },
   319	  });
   320	
   321	  if (!response.ok) {
   322	    const text = await response.text();
   323	    throw new Error(`Client token request failed (${response.status}): ${text.slice(0, 400)}`);
   324	  }
   325	
   326	  const payload = (await response.json()) as ClientTokenResponse;
   327	  const expiresAt = toExpiresAt(payload.expires_in);
   328	  return {
   329	    token: payload.client_token,
   330	    expiresAt,
   331	    lifetimeMs: Math.max(0, expiresAt - Date.now()),
   332	  };
   333	}
   334	
   335	async function refreshWithClientToken(clientToken: string, userId: string): Promise<TokenResponse> {
   336	  const body = new URLSearchParams({
   337	    grant_type: "urn:ietf:params:oauth:grant-type:client_token",
   338	    client_token: clientToken,
   339	    client_id: CLIENT_ID,
   340	    sub: userId,
   341	  });
   342	
   343	  const response = await fetch(TOKEN_ENDPOINT, {
   344	    method: "POST",
   345	    headers: {
   346	      "Content-Type": "application/x-www-form-urlencoded; charset=UTF-8",
   347	      Origin: "https://nvfile",
   348	      Accept: "application/json, text/plain, */*",
   349	      "User-Agent": GFN_USER_AGENT,
   350	    },
   351	    body,
   352	  });
   353	
   354	  if (!response.ok) {
   355	    const text = await response.text();
   356	    throw new Error(`Client-token refresh failed (${response.status}): ${text.slice(0, 400)}`);
   357	  }
   358	
   359	  return (await response.json()) as TokenResponse;
   360	}
   361	
   362	function mergeTokenSnapshot(base: AuthTokens, refreshed: TokenResponse): AuthTokens {
   363	  return {
   364	    accessToken: refreshed.access_token,
   365	    refreshToken: refreshed.refresh_token ?? base.refreshToken,
   366	    idToken: refreshed.id_token,
   367	    expiresAt: toExpiresAt(refreshed.expires_in),
   368	    clientToken: refreshed.client_token ?? base.clientToken,
   369	    clientTokenExpiresAt: base.clientTokenExpiresAt,
   370	    clientTokenLifetimeMs: base.clientTokenLifetimeMs,
   371	  };
   372	}
   373	
   374	function gravatarUrl(email: string, size = 80): string {
   375	  const normalized = email.trim().toLowerCase();
   376	  const hash = createHash("md5").update(normalized).digest("hex");
   377	  return `https://www.gravatar.com/avatar/${hash}?s=${size}&d=identicon`;
   378	}
   379	
   380	async function fetchUserInfo(tokens: AuthTokens): Promise<AuthUser> {
   381	  const jwtToken = tokens.idToken ?? tokens.accessToken;
   382	  const parsed = parseJwtPayload<{
   383	    sub?: string;
   384	    email?: string;
   385	    preferred_username?: string;
   386	    gfn_tier?: string;
   387	    picture?: string;
   388	  }>(jwtToken);
   389	
   390	  if (parsed?.sub) {
   391	    const emailFromToken = parsed.email;
   392	    const pictureFromToken = parsed.picture;
   393	    if (emailFromToken || pictureFromToken) {
   394	      const avatar = pictureFromToken ?? (emailFromToken ? gravatarUrl(emailFromToken) : undefined);
   395	      return {
   396	        userId: parsed.sub,
   397	        displayName: parsed.preferred_username ?? emailFromToken?.split("@")[0] ?? "User",
   398	        email: emailFromToken,
   399	        avatarUrl: avatar,
   400	        membershipTier: parsed.gfn_tier ?? "FREE",
   401	      };
   402	    }
   403	  }
   404	
   405	  const response = await fetch(USERINFO_ENDPOINT, {
   406	    headers: {
   407	      Authorization: `Bearer ${tokens.accessToken}`,
   408	      Origin: "https://nvfile",
   409	      Accept: "application/json",
   410	      "User-Agent": GFN_USER_AGENT,
   411	    },
   412	  });
   413	
   414	  if (!response.ok) {
   415	    throw new Error(`User info failed (${response.status})`);
   416	  }
   417	
   418	  const payload = (await response.json()) as {
   419	    sub: string;
   420	    preferred_username?: string;
   421	    email?: string;
   422	    picture?: string;
   423	  };
   424	
   425	  const email = payload.email;
   426	  const avatar = payload.picture ?? (email ? gravatarUrl(email) : undefined);
   427	
   428	  return {
   429	    userId: payload.sub,
   430	    displayName: payload.preferred_username ?? email?.split("@")[0] ?? "User",
   431	    email,
   432	    avatarUrl: avatar,
   433	    membershipTier: "FREE",
   434	  };
   435	}
   436	
   437	export class AuthService {
   438	  private providers: LoginProvider[] = [];
   439	  private sessions = new Map<string, AuthSession>();
   440	  private activeUserId: string | null = null;
   441	  private selectedProvider: LoginProvider = defaultProvider();
   442	  private cachedSubscription: SubscriptionInfo | null = null;
   443	  private cachedVpcId: string | null = null;
   444	
   445	  constructor(private readonly statePath: string) {}
   446	
   447	  async initialize(): Promise<void> {
   448	    try {
   449	      await access(this.statePath);
   450	    } catch {
   451	      await mkdir(dirname(this.statePath), { recursive: true });
   452	      await this.persist();
   453	      return;
   454	    }
   455	
   456	    try {
   457	      const raw = await readFile(this.statePath, "utf8");
   458	      const parsed = JSON.parse(raw) as Partial<PersistedAuthState> & {
   459	        session?: AuthSession | null;
   460	      };
   461	      if (parsed.selectedProvider) {
   462	        this.selectedProvider = normalizeProvider(parsed.selectedProvider);
   463	      }
   464	
   465	      this.sessions.clear();
   466	      if (Array.isArray(parsed.sessions)) {
   467	        for (const persistedSession of parsed.sessions) {
   468	          if (!persistedSession?.user?.userId) {
   469	            continue;
   470	          }
   471	          this.sessions.set(persistedSession.user.userId, {
   472	            ...persistedSession,
   473	            provider: normalizeProvider(persistedSession.provider),
   474	          });
   475	        }
   476	      } else if (parsed.session?.user?.userId) {
   477	        this.sessions.set(parsed.session.user.userId, {
   478	          ...parsed.session,
   479	          provider: normalizeProvider(parsed.session.provider),
   480	        });
   481	      }
   482	
   483	      if (typeof parsed.activeUserId === "string" && this.sessions.has(parsed.activeUserId)) {
   484	        this.activeUserId = parsed.activeUserId;
   485	      } else {
   486	        this.activeUserId = this.sessions.keys().next().value ?? null;
   487	      }
   488	
   489	      const restoredSession = this.getSession();
   490	      if (restoredSession) {
   491	        this.selectedProvider = restoredSession.provider;
   492	        await this.enrichUserTier();
   493	        await this.persist();
   494	      }
   495	    } catch {
   496	      this.sessions.clear();
   497	      this.activeUserId = null;
   498	      this.selectedProvider = defaultProvider();
   499	      await this.persist();
   500	    }
   501	  }
   502	
   503	  private async persist(): Promise<void> {
   504	    const payload: PersistedAuthState = {
   505	      sessions: Array.from(this.sessions.values()),
   506	      activeUserId: this.activeUserId,
   507	      selectedProvider: this.selectedProvider,
   508	    };
   509	
   510	    await mkdir(dirname(this.statePath), { recursive: true });
   511	    await writeFile(this.statePath, JSON.stringify(payload, null, 2), "utf8");
   512	  }
   513	
   514	  private async ensureClientToken(tokens: AuthTokens, userId: string): Promise<AuthTokens> {
   515	    const hasUsableClientToken =
   516	      Boolean(tokens.clientToken) &&
   517	      !isNearExpiry(tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS);
   518	    if (hasUsableClientToken) {
   519	      return tokens;
   520	    }
   521	
   522	    if (isExpired(tokens.expiresAt)) {
   523	      return tokens;
   524	    }
   525	
   526	    const clientToken = await requestClientToken(tokens.accessToken);
   527	    return {
   528	      ...tokens,
   529	      clientToken: clientToken.token,
   530	      clientTokenExpiresAt: clientToken.expiresAt,
   531	      clientTokenLifetimeMs: clientToken.lifetimeMs,
   532	    };
   533	  }
   534	
   535	  async getProviders(): Promise<LoginProvider[]> {
   536	    if (this.providers.length > 0) {
   537	      return this.providers;
   538	    }
   539	
   540	    let response: Response;
   541	    try {
   542	      response = await fetch(SERVICE_URLS_ENDPOINT, {
   543	        headers: {
   544	          Accept: "application/json",
   545	          "User-Agent": GFN_USER_AGENT,
   546	        },
   547	      });
   548	    } catch (error) {
   549	      console.warn("Failed to fetch providers, using default:", error);
   550	      this.providers = [defaultProvider()];
   551	      return this.providers;
   552	    }
   553	
   554	    if (!response.ok) {
   555	      console.warn(`Providers fetch failed with status ${response.status}, using default`);
   556	      this.providers = [defaultProvider()];
   557	      return this.providers;
   558	    }
   559	
   560	    try {
   561	      const payload = (await response.json()) as ServiceUrlsResponse;
   562	      const endpoints = payload.gfnServiceInfo?.gfnServiceEndpoints ?? [];
   563	
   564	      const providers = endpoints
   565	        .map<LoginProvider>((entry) => ({
   566	          idpId: entry.idpId,
   567	          code: entry.loginProviderCode,
   568	          displayName:
   569	            entry.loginProviderCode === "BPC" ? "bro.game" : entry.loginProviderDisplayName,
   570	          streamingServiceUrl: entry.streamingServiceUrl,
   571	          priority: entry.loginProviderPriority ?? 0,
   572	        }))
   573	        .sort((a, b) => a.priority - b.priority)
   574	        .map(normalizeProvider);
   575	
   576	      this.providers = providers.length > 0 ? providers : [defaultProvider()];
   577	      console.log(`Loaded ${this.providers.length} providers`);
   578	      return this.providers;
   579	    } catch (error) {
   580	      console.warn("Failed to parse providers response, using default:", error);
   581	      this.providers = [defaultProvider()];
   582	      return this.providers;
   583	    }
   584	  }
   585	
   586	  setSession(session: AuthSession | null): void {
   587	    if (!session) {
   588	      this.sessions.clear();
   589	      this.activeUserId = null;
   590	      this.selectedProvider = defaultProvider();
   591	      this.clearSubscriptionCache();
   592	      this.clearVpcCache();
   593	      void this.persist();
   594	      return;
   595	    }
   596	
   597	    const normalized: AuthSession = {
   598	      ...session,
   599	      provider: normalizeProvider(session.provider),
   600	    };
   601	    this.sessions.set(normalized.user.userId, normalized);
   602	    this.activeUserId = normalized.user.userId;
   603	    this.selectedProvider = normalized.provider;
   604	    this.clearSubscriptionCache();
   605	    this.clearVpcCache();
   606	    void this.persist();
   607	  }
   608	
   609	  getSession(): AuthSession | null {
   610	    if (!this.activeUserId) {
   611	      return null;
   612	    }
   613	    return this.sessions.get(this.activeUserId) ?? null;
   614	  }
   615	
   616	  getSavedAccounts(): SavedAccount[] {
   617	    return Array.from(this.sessions.values()).map((session) => ({
   618	      userId: session.user.userId,
   619	      displayName: session.user.displayName,
   620	      email: session.user.email,
   621	      avatarUrl: session.user.avatarUrl,
   622	      membershipTier: session.user.membershipTier,
   623	      providerCode: session.provider.code,
   624	    }));
   625	  }
   626	
   627	  async switchAccount(userId: string): Promise<AuthSession> {
   628	    const target = this.sessions.get(userId);
   629	    if (!target) {
   630	      throw new Error("Saved account not found");
   631	    }
   632	    this.activeUserId = userId;
   633	    this.selectedProvider = target.provider;
   634	    this.clearSubscriptionCache();
   635	    this.clearVpcCache();
   636	
   637	    const result = await this.ensureValidSessionWithStatus(true, userId);
   638	    const refreshFailed =
   639	      result.refresh.outcome === "failed" || result.refresh.outcome === "missing_refresh_token";
   640	    const switchedUserMismatch = result.session?.user.userId !== userId;
   641	    if (!result.session || refreshFailed || switchedUserMismatch) {
   642	      await this.removeAccount(userId);
   643	      const fallbackMessage = "Failed to switch account due to an invalid or expired session.";
   644	      if (switchedUserMismatch) {
   645	        throw new Error("Switched session did not match the selected account.");
   646	      }
   647	      if (result.refresh.outcome === "missing_refresh_token") {
   648	        throw new Error("Saved login for this account is incomplete. Please log in to this account again.");
   649	      }
   650	      throw new Error(result.refresh.message || fallbackMessage);
   651	    }
   652	    return result.session;
   653	  }
   654	
   655	  async removeAccount(userId: string): Promise<void> {
   656	    const removed = this.sessions.delete(userId);
   657	    if (!removed) {
   658	      return;
   659	    }
   660	    if (this.activeUserId === userId) {
   661	      this.activeUserId = this.sessions.keys().next().value ?? null;
   662	    }
   663	    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
   664	    this.clearSubscriptionCache();
   665	    this.clearVpcCache();
   666	    await this.persist();
   667	  }
   668	
   669	  async logoutAll(): Promise<void> {
   670	    this.sessions.clear();
   671	    this.activeUserId = null;
   672	    this.selectedProvider = defaultProvider();
   673	    this.cachedSubscription = null;
   674	    this.clearVpcCache();
   675	    await this.persist();
   676	  }
   677	
   678	  getSelectedProvider(): LoginProvider {
   679	    return this.getSession()?.provider ?? this.selectedProvider;
   680	  }
   681	
   682	  async getRegions(explicitToken?: string): Promise<StreamRegion[]> {
   683	    const provider = this.getSelectedProvider();
   684	    const base = provider.streamingServiceUrl.endsWith("/")
   685	      ? provider.streamingServiceUrl
   686	      : `${provider.streamingServiceUrl}/`;
   687	
   688	    let token = explicitToken;
   689	    if (!token) {
   690	      const session = await this.ensureValidSession();
   691	      token = session ? session.tokens.idToken ?? session.tokens.accessToken : undefined;
   692	    }
   693	
   694	    const headers: Record<string, string> = {
   695	      Accept: "application/json",
   696	      "nv-client-id": "ec7e38d4-03af-4b58-b131-cfb0495903ab",
   697	      "nv-client-type": "BROWSER",
   698	      "nv-client-version": "2.0.80.173",
   699	      "nv-client-streamer": "WEBRTC",
   700	      "nv-device-os": "WINDOWS",
   701	      "nv-device-type": "DESKTOP",
   702	      "User-Agent": GFN_USER_AGENT,
   703	    };
   704	
   705	    if (token) {
   706	      headers.Authorization = `GFNJWT ${token}`;
   707	    }
   708	
   709	    let response: Response;
   710	    try {
   711	      response = await fetch(`${base}v2/serverInfo`, {
   712	        headers,
   713	      });
   714	    } catch {
   715	      return [];
   716	    }
   717	
   718	    if (!response.ok) {
   719	      return [];
   720	    }
   721	
   722	    const payload = (await response.json()) as ServerInfoResponse;
   723	    const regions = (payload.metaData ?? [])
   724	      .filter((entry) => entry.value.startsWith("https://"))
   725	      .filter((entry) => entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-"))
   726	      .map<StreamRegion>((entry) => ({
   727	        name: entry.key,
   728	        url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`,
   729	      }))
   730	      .sort((a, b) => a.name.localeCompare(b.name));
   731	
   732	    return regions;
   733	  }
   734	
   735	  async login(input: AuthLoginRequest): Promise<AuthSession> {
   736	    const providers = await this.getProviders();
   737	    const selected =
   738	      providers.find((provider) => provider.idpId === input.providerIdpId) ??
   739	      this.selectedProvider ??
   740	      providers[0] ??
   741	      defaultProvider();
   742	
   743	    this.selectedProvider = normalizeProvider(selected);
   744	
   745	    const { verifier, challenge } = generatePkce();
   746	    const port = await findAvailablePort();
   747	    const authUrl = buildAuthUrl(this.selectedProvider, challenge, port);
   748	
   749	    const codePromise = waitForAuthorizationCode(port, 120000);
   750	    await shell.openExternal(authUrl);
   751	    const code = await codePromise;
   752	
   753	    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
   754	    const user = await fetchUserInfo(initialTokens);
   755	    console.debug("auth: fetched user info during login", { userId: user.userId, email: user.email, avatarUrl: user.avatarUrl });
   756	    let tokens = initialTokens;
   757	    try {
   758	      tokens = await this.ensureClientToken(initialTokens, user.userId);
   759	    } catch (error) {
   760	      console.warn("Unable to fetch client token after login. Falling back to OAuth token only:", error);
   761	    }
   762	
   763	    const nextSession: AuthSession = {
   764	      provider: this.selectedProvider,
   765	      tokens,
   766	      user,
   767	    };
   768	    this.sessions.set(user.userId, nextSession);
   769	    this.activeUserId = user.userId;
   770	    this.selectedProvider = nextSession.provider;
   771	    this.clearSubscriptionCache();
   772	    this.clearVpcCache();
   773	
   774	    // Fetch real membership tier from MES subscription API
   775	    // (JWT does not contain gfn_tier, so fetchUserInfo always falls back to "FREE")
   776	    await this.enrichUserTier();
   777	
   778	    await this.persist();
   779	    return this.getSession() as AuthSession;
   780	  }
   781	
   782	  async logout(): Promise<void> {
   783	    if (!this.activeUserId) {
   784	      return;
   785	    }
   786	    this.sessions.delete(this.activeUserId);
   787	    this.activeUserId = this.sessions.keys().next().value ?? null;
   788	    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
   789	    this.cachedSubscription = null;
   790	    this.clearVpcCache();
   791	    await this.persist();
   792	  }
   793	
   794	  /**
   795	   * Fetch subscription info for the current user.
   796	   * Uses caching - call clearSubscriptionCache() to force refresh.
   797	   */
   798	  async getSubscription(): Promise<SubscriptionInfo | null> {
   799	    // Return cached subscription if available
   800	    if (this.cachedSubscription) {
   801	      return this.cachedSubscription;
   802	    }
   803	
   804	    const session = await this.ensureValidSession();
   805	    if (!session) {
   806	      return null;
   807	    }
   808	
   809	    const token = session.tokens.idToken ?? session.tokens.accessToken;
   810	    const userId = session.user.userId;
   811	
   812	    // Fetch dynamic regions to get the VPC ID (handles Alliance partners correctly)
   813	    const { vpcId } = await fetchDynamicRegions(token, session.provider.streamingServiceUrl);
   814	
   815	    const subscription = await fetchSubscription(token, userId, vpcId ?? undefined);
   816	    this.cachedSubscription = subscription;
   817	    return subscription;
   818	  }
   819	
   820	  /**
   821	   * Clear the cached subscription info.
   822	   * Called automatically on logout.
   823	   */
   824	  clearSubscriptionCache(): void {
   825	    this.cachedSubscription = null;
   826	  }
   827	
   828	  /**
   829	   * Get the cached subscription without fetching.
   830	   * Returns null if not cached.
   831	   */
   832	  getCachedSubscription(): SubscriptionInfo | null {
   833	    return this.cachedSubscription;
   834	  }
   835	
   836	  /**
   837	   * Get the VPC ID for the current provider.
   838	   * Returns cached value if available, otherwise fetches from serverInfo endpoint.
   839	   * The VPC ID is used for Alliance partner support and routing to correct data center.
   840	   */
   841	  async getVpcId(explicitToken?: string): Promise<string | null> {
   842	    // Return cached VPC ID if available
   843	    if (this.cachedVpcId) {
   844	      return this.cachedVpcId;
   845	    }
   846	
   847	    const provider = this.getSelectedProvider();
   848	    const base = provider.streamingServiceUrl.endsWith("/")
   849	      ? provider.streamingServiceUrl
   850	      : `${provider.streamingServiceUrl}/`;
   851	
   852	    let token = explicitToken;
   853	    if (!token) {
   854	      const session = await this.ensureValidSession();
   855	      token = session ? session.tokens.idToken ?? session.tokens.accessToken : undefined;
   856	    }
   857	
   858	    const headers: Record<string, string> = {
   859	      Accept: "application/json",
   860	      "nv-client-id": "ec7e38d4-03af-4b58-b131-cfb0495903ab",
   861	      "nv-client-type": "BROWSER",
   862	      "nv-client-version": "2.0.80.173",
   863	      "nv-client-streamer": "WEBRTC",
   864	      "nv-device-os": "WINDOWS",
   865	      "nv-device-type": "DESKTOP",
   866	      "User-Agent": GFN_USER_AGENT,
   867	    };
   868	
   869	    if (token) {
   870	      headers.Authorization = `GFNJWT ${token}`;
   871	    }
   872	
   873	    try {
   874	      const response = await fetch(`${base}v2/serverInfo`, {
   875	        headers,
   876	      });
   877	
   878	      if (!response.ok) {
   879	        return null;
   880	      }
   881	
   882	      const payload = (await response.json()) as ServerInfoResponse;
   883	      const vpcId = payload.requestStatus?.serverId ?? null;
   884	
   885	      // Cache the VPC ID
   886	      if (vpcId) {
   887	        this.cachedVpcId = vpcId;
   888	      }
   889	
   890	      return vpcId;
   891	    } catch {
   892	      return null;
   893	    }
   894	  }
   895	
   896	  /**
   897	   * Clear the cached VPC ID.
   898	   * Called automatically on logout.
   899	   */
   900	  clearVpcCache(): void {
   901	    this.cachedVpcId = null;
   902	  }
   903	
   904	  /**
   905	   * Get the cached VPC ID without fetching.
   906	   * Returns null if not cached.
   907	   */
   908	  getCachedVpcId(): string | null {
   909	    return this.cachedVpcId;
   910	  }
   911	
   912	  /**
   913	   * Enrich the current session's user with the real membership tier from MES API.
   914	   * Falls back silently to the existing tier if the fetch fails.
   915	   */
   916	  private async enrichUserTier(): Promise<void> {
   917	    const session = this.getSession();
   918	    if (!session) return;
   919	
   920	    try {
   921	      const subscription = await this.getSubscription();
   922	      if (subscription && subscription.membershipTier) {
   923	        this.sessions.set(session.user.userId, {
   924	          ...session,
   925	          user: {
   926	            ...session.user,
   927	            membershipTier: subscription.membershipTier,
   928	          },
   929	        });
   930	        console.log(`Resolved membership tier: ${subscription.membershipTier}`);
   931	      }
   932	    } catch (error) {
   933	      console.warn("Failed to fetch subscription tier, keeping fallback:", error);
   934	    }
   935	  }
   936	
   937	  private shouldRefresh(tokens: AuthTokens): boolean {
   938	    return isNearExpiry(tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS);
   939	  }
   940	
   941	  async ensureValidSessionWithStatus(
   942	    forceRefresh = false,
   943	    expectedUserId?: string,
   944	  ): Promise<AuthSessionResult> {
   945	    const currentSession = this.getSession();
   946	    if (!currentSession) {
   947	      return {
   948	        session: null,
   949	        refresh: {
   950	          attempted: false,
   951	          forced: forceRefresh,
   952	          outcome: "not_attempted",
   953	          message: "No saved session found.",
   954	        },
   955	      };
   956	    }
   957	
   958	    const userId = currentSession.user.userId;
   959	    let tokens = currentSession.tokens;
   960	
   961	    // Official GFN client flow relies on client_token-based refresh. Bootstrap it
   962	    // for older sessions that were saved before we persisted client tokens.
   963	    if (!tokens.clientToken && !isExpired(tokens.expiresAt)) {
   964	      try {
   965	        const withClientToken = await this.ensureClientToken(tokens, userId);
   966	        if (withClientToken.clientToken && withClientToken.clientToken !== tokens.clientToken) {
   967	          this.sessions.set(userId, {
   968	            ...currentSession,
   969	            tokens: withClientToken,
   970	          });
   971	          tokens = withClientToken;
   972	          await this.persist();
   973	        }
   974	      } catch (error) {
   975	        console.warn("Unable to bootstrap client token from saved session:", error);
   976	      }
   977	    }
   978	
   979	    const shouldRefreshNow = forceRefresh || this.shouldRefresh(tokens);
   980	    if (!shouldRefreshNow) {
   981	      return {
   982	        session: this.getSession(),
   983	        refresh: {
   984	          attempted: false,
   985	          forced: forceRefresh,
   986	          outcome: "not_attempted",
   987	          message: "Session token is still valid.",
   988	        },
   989	      };
   990	    }
   991	
   992	    const applyRefreshedTokens = async (
   993	      refreshedTokens: AuthTokens,
   994	      source: "client_token" | "refresh_token",
   995	    ): Promise<AuthSessionResult> => {
   996	      const latestSession = this.getSession() ?? currentSession;
   997	      let refreshedUser: AuthUser | null = null;
   998	      let userInfoError: string | undefined;
   999	      try {
  1000	        refreshedUser = await fetchUserInfo(refreshedTokens);
  1001	        console.debug("auth: fetched user info on token refresh", {
  1002	          userId: refreshedUser.userId,
  1003	          email: refreshedUser.email,
  1004	          avatarUrl: refreshedUser.avatarUrl,
  1005	        });
  1006	      } catch (error) {
  1007	        console.warn("Token refresh succeeded but user info refresh failed. Keeping cached user:", error);
  1008	        userInfoError = error instanceof Error ? error.message : "Unknown error while fetching user info";
  1009	      }
  1010	
  1011	      if (expectedUserId && !refreshedUser) {
  1012	        return {
  1013	          session: latestSession,
  1014	          refresh: {
  1015	            attempted: true,
  1016	            forced: forceRefresh,
  1017	            outcome: "failed",
  1018	            message: "Token refresh could not verify the expected account identity.",
  1019	            error: userInfoError ?? `expected_user_id:${expectedUserId} user_info_unavailable`,
  1020	          },
  1021	        };
  1022	      }
  1023	
  1024	      if (expectedUserId && refreshedUser && refreshedUser.userId !== expectedUserId) {
  1025	        return {
  1026	          session: latestSession,
  1027	          refresh: {
  1028	            attempted: true,
  1029	            forced: forceRefresh,
  1030	            outcome: "failed",
  1031	            message: "Token refresh returned a different account than expected.",
  1032	            error: `expected_user_id:${expectedUserId} actual_user_id:${refreshedUser.userId}`,
  1033	          },
  1034	        };
  1035	      }
  1036	
  1037	      const resolvedUser = refreshedUser ?? latestSession.user;
  1038	      const updatedSession: AuthSession = {
  1039	        provider: latestSession.provider,
  1040	        tokens: refreshedTokens,
  1041	        user: resolvedUser,
  1042	      };
  1043	      this.sessions.set(updatedSession.user.userId, updatedSession);
  1044	
  1045	      // Re-fetch real tier after token refresh
  1046	      this.clearSubscriptionCache();
  1047	      await this.enrichUserTier();
  1048	      await this.persist();
  1049	
  1050	      const sourceText = source === "client_token" ? "client token" : "refresh token";
  1051	      return {
  1052	        session: this.getSession(),
  1053	        refresh: {
  1054	          attempted: true,
  1055	          forced: forceRefresh,
  1056	          outcome: "refreshed",
  1057	          message: forceRefresh
  1058	            ? `Saved session token refreshed via ${sourceText}.`
  1059	            : `Session token refreshed via ${sourceText} because it was near expiry.`,
  1060	        },
  1061	      };
  1062	    };
  1063	
  1064	    const refreshErrors: string[] = [];
  1065	
  1066	    if (tokens.clientToken) {
  1067	      try {
  1068	        const refreshedFromClientToken = await refreshWithClientToken(tokens.clientToken, userId);
  1069	        let refreshedTokens = mergeTokenSnapshot(tokens, refreshedFromClientToken);
  1070	        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
  1071	        return applyRefreshedTokens(refreshedTokens, "client_token");
  1072	      } catch (error) {
  1073	        const message =
  1074	          error instanceof Error ? error.message : "Unknown error while refreshing with client token";
  1075	        refreshErrors.push(`client_token: ${message}`);
  1076	      }
  1077	    }
  1078	
  1079	    if (tokens.refreshToken) {
  1080	      try {
  1081	        const refreshedOAuth = await refreshAuthTokens(tokens.refreshToken);
  1082	        let refreshedTokens: AuthTokens = {
  1083	          ...tokens,
  1084	          ...refreshedOAuth,
  1085	          // OAuth refresh does not always return a new client token.
  1086	          clientToken: tokens.clientToken,
  1087	          clientTokenExpiresAt: tokens.clientTokenExpiresAt,
  1088	          clientTokenLifetimeMs: tokens.clientTokenLifetimeMs,
  1089	        };
  1090	        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
  1091	        return applyRefreshedTokens(refreshedTokens, "refresh_token");
  1092	      } catch (error) {
  1093	        const message =
  1094	          error instanceof Error ? error.message : "Unknown error while refreshing token";
  1095	        refreshErrors.push(`refresh_token: ${message}`);
  1096	      }
  1097	    }
  1098	
  1099	    const errorText = refreshErrors.length > 0 ? refreshErrors.join(" | ") : undefined;
  1100	    const expired = isExpired(tokens.expiresAt);
  1101	
  1102	    if (!tokens.clientToken && !tokens.refreshToken) {
  1103	      if (expired) {
  1104	        await this.logout();
  1105	        return {
  1106	          session: null,
  1107	          refresh: {
  1108	            attempted: true,
  1109	            forced: forceRefresh,
  1110	            outcome: "missing_refresh_token",
  1111	            message: "Saved session expired and has no refresh mechanism. Please log in again.",
  1112	          },
  1113	        };
  1114	      }
  1115	
  1116	      return {
  1117	        session: this.getSession(),
  1118	        refresh: {
  1119	          attempted: true,
  1120	          forced: forceRefresh,
  1121	          outcome: "missing_refresh_token",
  1122	          message: "No refresh token available. Using saved session token.",
  1123	        },
  1124	      };
  1125	    }
  1126	
  1127	    if (expired) {
  1128	      await this.logout();
  1129	      return {
  1130	        session: this.getSession(),
  1131	        refresh: {
  1132	          attempted: true,
  1133	          forced: forceRefresh,
  1134	          outcome: "failed",
  1135	          message: "Token refresh failed and the saved session expired. Please log in again.",
  1136	          error: errorText,
  1137	        },
  1138	      };
  1139	    }
  1140	
  1141	    return {
  1142	      session: this.getSession(),
  1143	      refresh: {
  1144	        attempted: true,
  1145	        forced: forceRefresh,
  1146	        outcome: "failed",
  1147	        message: "Token refresh failed. Using saved session token.",
  1148	        error: errorText,
  1149	      },
  1150	    };
  1151	  }
  1152	
  1153	  async ensureValidSession(): Promise<AuthSession | null> {
  1154	    const result = await this.ensureValidSessionWithStatus(false);
  1155	    return result.session;
  1156	  }
  1157	
  1158	  async resolveJwtToken(explicitToken?: string): Promise<string> {
  1159	    // Prefer the managed auth session whenever it exists so renderer-side cached
  1160	    // tokens cannot bypass refresh logic.
  1161	    if (this.getSession()) {
  1162	      const session = await this.ensureValidSession();
  1163	      if (!session) {
  1164	        throw new Error("No authenticated session available");
  1165	      }
  1166	      return session.tokens.idToken ?? session.tokens.accessToken;
  1167	    }
  1168	
  1169	    if (explicitToken && explicitToken.trim()) {
  1170	      return explicitToken.trim();
  1171	    }
  1172	
  1173	    const session = await this.ensureValidSession();
  1174	    if (!session) {
  1175	      throw new Error("No authenticated session available");
  1176	    }
  1177	
  1178	    return session.tokens.idToken ?? session.tokens.accessToken;
  1179	  }
  1180	}
  1181	