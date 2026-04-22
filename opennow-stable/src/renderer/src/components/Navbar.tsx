     1	import type { ActiveSessionInfo, AuthUser, SavedAccount, SubscriptionInfo } from "@shared/gfn";
     2	import { House, Library, Settings, User, Zap, Timer, HardDrive, X, Loader2, PlayCircle, Square, ChevronDown, Check, Plus } from "lucide-react";
     3	import { useEffect, useRef, useState, type JSX } from "react";
     4	import { createPortal } from "react-dom";
     5	
     6	interface NavbarProps {
     7	  currentPage: "home" | "library" | "settings";
     8	  onNavigate: (page: "home" | "library" | "settings") => void;
     9	  user: AuthUser | null;
    10	  subscription: SubscriptionInfo | null;
    11	  activeSession: ActiveSessionInfo | null;
    12	  activeSessionGameTitle: string | null;
    13	  isResumingSession: boolean;
    14	  isTerminatingSession: boolean;
    15	  onResumeSession: () => void;
    16	  onTerminateSession: () => void;
    17	  savedAccounts: SavedAccount[];
    18	  onSwitchAccount: (userId: string) => void;
    19	  onRemoveAccount: (userId: string) => void;
    20	  onAddAccount: () => void;
    21	  onLogout: () => void;
    22	}
    23	
    24	type NavbarModalType = "time" | "storage" | null;
    25	
    26	function getTierDisplay(tier: string): { label: string; className: string } {
    27	  const t = tier.toUpperCase();
    28	  if (t === "ULTIMATE") return { label: "Ultimate", className: "tier-ultimate" };
    29	  if (t === "PRIORITY" || t === "PERFORMANCE") return { label: "Priority", className: "tier-priority" };
    30	  return { label: "Free", className: "tier-free" };
    31	}
    32	
    33	export function Navbar({
    34	  currentPage,
    35	  onNavigate,
    36	  user,
    37	  subscription,
    38	  activeSession,
    39	  activeSessionGameTitle,
    40	  isResumingSession,
    41	  isTerminatingSession,
    42	  onResumeSession,
    43	  onTerminateSession,
    44	  savedAccounts,
    45	  onSwitchAccount,
    46	  onRemoveAccount,
    47	  onAddAccount,
    48	  onLogout,
    49	}: NavbarProps): JSX.Element {
    50	  const [modalType, setModalType] = useState<NavbarModalType>(null);
    51	  const [accountDropdownOpen, setAccountDropdownOpen] = useState(false);
    52	  const accountContainerRef = useRef<HTMLDivElement | null>(null);
    53	
    54	  const navItems = [
    55	    { id: "home" as const, label: "Store", icon: House },
    56	    { id: "library" as const, label: "Library", icon: Library },
    57	    { id: "settings" as const, label: "Settings", icon: Settings },
    58	  ];
    59	
    60	  const tierInfo = user ? getTierDisplay(user.membershipTier) : null;
    61	  const formatHours = (value: number): string => {
    62	    if (!Number.isFinite(value)) return "0";
    63	    const rounded = value >= 10 ? Math.round(value) : Math.round(value * 10) / 10;
    64	    return Number.isInteger(rounded) ? String(rounded) : rounded.toFixed(1);
    65	  };
    66	  const formatGb = (value: number): string => {
    67	    if (!Number.isFinite(value)) return "0";
    68	    return Number.isInteger(value) ? String(value) : value.toFixed(1);
    69	  };
    70	  const formatPercent = (value: number): string => {
    71	    if (!Number.isFinite(value)) return "0%";
    72	    const rounded = Math.max(0, Math.min(100, Math.round(value)));
    73	    return `${rounded}%`;
    74	  };
    75	  const formatDateTime = (value: string | undefined): string | null => {
    76	    if (!value) return null;
    77	    const parsed = new Date(value);
    78	    if (Number.isNaN(parsed.getTime())) return null;
    79	    return parsed.toLocaleString();
    80	  };
    81	  const clamp = (value: number): number => Math.min(1, Math.max(0, value));
    82	  const toneByLeftRatio = (ratio: number): "good" | "warn" | "critical" => {
    83	    if (ratio <= 0.15) return "critical";
    84	    if (ratio <= 0.4) return "warn";
    85	    return "good";
    86	  };
    87	
    88	  const timeTotal = subscription?.totalHours ?? 0;
    89	  const timeLeft = subscription?.remainingHours ?? 0;
    90	  const timeUsed = subscription?.usedHours ?? Math.max(timeTotal - timeLeft, 0);
    91	  const allottedHours = subscription?.allottedHours ?? 0;
    92	  const purchasedHours = subscription?.purchasedHours ?? 0;
    93	  const rolledOverHours = subscription?.rolledOverHours ?? 0;
    94	  const timeUsedRatio =
    95	    subscription && !subscription.isUnlimited && timeTotal > 0 ? clamp(timeUsed / timeTotal) : 0;
    96	  const timeLeftRatio =
    97	    subscription && !subscription.isUnlimited && timeTotal > 0 ? clamp(timeLeft / timeTotal) : 1;
    98	  const timeTone: "good" | "warn" | "critical" = subscription?.isUnlimited
    99	    ? "good"
   100	    : toneByLeftRatio(timeLeftRatio);
   101	  const timeLabel = subscription
   102	    ? subscription.isUnlimited
   103	      ? "Unlimited time"
   104	      : `${formatHours(timeLeft)}h left`
   105	    : null;
   106	
   107	  const storageTotal = subscription?.storageAddon?.sizeGb;
   108	  const storageUsed = subscription?.storageAddon?.usedGb;
   109	  const storageHasData = storageTotal !== undefined && storageUsed !== undefined;
   110	  const storageLeft =
   111	    storageHasData
   112	      ? Math.max(storageTotal - storageUsed, 0)
   113	      : undefined;
   114	  const storageUsedRatio =
   115	    storageHasData && storageTotal > 0 ? clamp(storageUsed / storageTotal) : 0;
   116	  const storageLeftRatio =
   117	    storageHasData && storageTotal > 0 ? clamp((storageLeft ?? 0) / storageTotal) : 1;
   118	  const storageTone = toneByLeftRatio(storageLeftRatio);
   119	  const storageLabel =
   120	    storageHasData
   121	      ? `${formatGb(storageLeft ?? 0)} GB left`
   122	      : storageTotal !== undefined
   123	        ? `${formatGb(storageTotal)} GB total`
   124	        : null;
   125	
   126	  const spanStart = formatDateTime(subscription?.currentSpanStartDateTime);
   127	  const spanEnd = formatDateTime(subscription?.currentSpanEndDateTime);
   128	  const firstEntitlementStart = formatDateTime(subscription?.firstEntitlementStartDateTime);
   129	  const modalTitle = modalType === "time" ? "Playtime Details" : "Storage Details";
   130	  const activeSessionTitle = activeSessionGameTitle?.trim() || null;
   131	  const activeUserId = user?.userId ?? null;
   132	
   133	  useEffect(() => {
   134	    if (!accountDropdownOpen) return;
   135	    const onDocumentPointerDown = (event: MouseEvent) => {
   136	      if (!accountContainerRef.current?.contains(event.target as Node)) {
   137	        setAccountDropdownOpen(false);
   138	      }
   139	    };
   140	    window.addEventListener("mousedown", onDocumentPointerDown);
   141	    return () => window.removeEventListener("mousedown", onDocumentPointerDown);
   142	  }, [accountDropdownOpen]);
   143	
   144	  useEffect(() => {
   145	    if (!modalType) return;
   146	    const onKeyDown = (event: KeyboardEvent) => {
   147	      if (event.key === "Escape") {
   148	        setModalType(null);
   149	      }
   150	    };
   151	    window.addEventListener("keydown", onKeyDown);
   152	    const previousOverflow = document.body.style.overflow;
   153	    document.body.style.overflow = "hidden";
   154	    return () => {
   155	      window.removeEventListener("keydown", onKeyDown);
   156	      document.body.style.overflow = previousOverflow;
   157	    };
   158	  }, [modalType]);
   159	
   160	  const modal = modalType && subscription
   161	    ? createPortal(
   162	        <div className="navbar-modal-backdrop" onClick={() => setModalType(null)}>
   163	          <div
   164	            className="navbar-modal"
   165	            role="dialog"
   166	            aria-modal="true"
   167	            aria-label={modalTitle}
   168	            onClick={(event) => event.stopPropagation()}
   169	          >
   170	            <div className="navbar-modal-header">
   171	              <h3>{modalTitle}</h3>
   172	              <button
   173	                type="button"
   174	                className="navbar-modal-close"
   175	                onClick={() => setModalType(null)}
   176	                title="Close"
   177	              >
   178	                <X size={16} />
   179	              </button>
   180	            </div>
   181	
   182	            {modalType === "time" && (
   183	              <div className="navbar-modal-body">
   184	                {!subscription.isUnlimited && timeTotal > 0 && (
   185	                  <div className="navbar-meter">
   186	                    <div className="navbar-meter-head">
   187	                      <span>Time Usage</span>
   188	                      <strong>{formatPercent(timeUsedRatio * 100)} used</strong>
   189	                    </div>
   190	                    <div className="navbar-meter-track">
   191	                      <span
   192	                        className={`navbar-meter-fill navbar-meter-fill--${timeTone}`}
   193	                        style={{ width: `${timeUsedRatio * 100}%` }}
   194	                      />
   195	                    </div>
   196	                    <div className="navbar-meter-legend">
   197	                      <span>{formatHours(timeUsed)}h used</span>
   198	                      <span>{formatHours(timeLeft)}h left</span>
   199	                    </div>
   200	                  </div>
   201	                )}
   202	                <div className="navbar-modal-row"><span>Tier</span><strong>{subscription.membershipTier}</strong></div>
   203	                {subscription.subscriptionType && (
   204	                  <div className="navbar-modal-row"><span>Type</span><strong>{subscription.subscriptionType}</strong></div>
   205	                )}
   206	                {subscription.subscriptionSubType && (
   207	                  <div className="navbar-modal-row"><span>Sub Type</span><strong>{subscription.subscriptionSubType}</strong></div>
   208	                )}
   209	                <div className="navbar-modal-row"><span>Time Left</span><strong>{subscription.isUnlimited ? "Unlimited" : `${formatHours(timeLeft)}h`}</strong></div>
   210	                <div className="navbar-modal-row"><span>Total Time</span><strong>{subscription.isUnlimited ? "Unlimited" : `${formatHours(timeTotal)}h`}</strong></div>
   211	                <div className="navbar-modal-row"><span>Used Time</span><strong>{formatHours(timeUsed)}h</strong></div>
   212	                <div className="navbar-modal-row"><span>Allotted</span><strong>{formatHours(allottedHours)}h</strong></div>
   213	                <div className="navbar-modal-row"><span>Purchased</span><strong>{formatHours(purchasedHours)}h</strong></div>
   214	                <div className="navbar-modal-row"><span>Rolled Over</span><strong>{formatHours(rolledOverHours)}h</strong></div>
   215	                {firstEntitlementStart && (
   216	                  <div className="navbar-modal-row"><span>First Entitlement</span><strong>{firstEntitlementStart}</strong></div>
   217	                )}
   218	                {spanStart && <div className="navbar-modal-row"><span>Period Start</span><strong>{spanStart}</strong></div>}
   219	                {spanEnd && <div className="navbar-modal-row"><span>Period End</span><strong>{spanEnd}</strong></div>}
   220	                {subscription.notifyUserWhenTimeRemainingInMinutes !== undefined && (
   221	                  <div className="navbar-modal-row"><span>Notify At (General)</span><strong>{subscription.notifyUserWhenTimeRemainingInMinutes} min</strong></div>
   222	                )}
   223	                {subscription.notifyUserOnSessionWhenRemainingTimeInMinutes !== undefined && (
   224	                  <div className="navbar-modal-row"><span>Notify At (In Session)</span><strong>{subscription.notifyUserOnSessionWhenRemainingTimeInMinutes} min</strong></div>
   225	                )}
   226	                {subscription.state && <div className="navbar-modal-row"><span>Plan State</span><strong>{subscription.state}</strong></div>}
   227	                {subscription.isGamePlayAllowed !== undefined && (
   228	                  <div className="navbar-modal-row"><span>Gameplay Allowed</span><strong>{subscription.isGamePlayAllowed ? "Yes" : "No"}</strong></div>
   229	                )}
   230	              </div>
   231	            )}
   232	
   233	            {modalType === "storage" && (
   234	              <div className="navbar-modal-body">
   235	                {storageHasData && (
   236	                  <div className="navbar-meter">
   237	                    <div className="navbar-meter-head">
   238	                      <span>Storage Usage</span>
   239	                      <strong>{formatPercent(storageUsedRatio * 100)} used</strong>
   240	                    </div>
   241	                    <div className="navbar-meter-track">
   242	                      <span
   243	                        className={`navbar-meter-fill navbar-meter-fill--${storageTone}`}
   244	                        style={{ width: `${storageUsedRatio * 100}%` }}
   245	                      />
   246	                    </div>
   247	                    <div className="navbar-meter-legend">
   248	                      <span>{formatGb(storageUsed ?? 0)} GB used</span>
   249	                      <span>{formatGb(storageLeft ?? 0)} GB left</span>
   250	                    </div>
   251	                  </div>
   252	                )}
   253	                <div className="navbar-modal-row"><span>Storage Left</span><strong>{storageLeft !== undefined ? `${formatGb(storageLeft)} GB` : "N/A"}</strong></div>
   254	                <div className="navbar-modal-row"><span>Storage Used</span><strong>{storageUsed !== undefined ? `${formatGb(storageUsed)} GB` : "N/A"}</strong></div>
   255	                <div className="navbar-modal-row"><span>Storage Total</span><strong>{storageTotal !== undefined ? `${formatGb(storageTotal)} GB` : "N/A"}</strong></div>
   256	                {subscription.storageAddon?.regionName && (
   257	                  <div className="navbar-modal-row"><span>Storage Region</span><strong>{subscription.storageAddon.regionName}</strong></div>
   258	                )}
   259	                {subscription.storageAddon?.regionCode && (
   260	                  <div className="navbar-modal-row"><span>Storage Region Code</span><strong>{subscription.storageAddon.regionCode}</strong></div>
   261	                )}
   262	                {subscription.serverRegionId && (
   263	                  <div className="navbar-modal-row"><span>Server Region (VPC)</span><strong>{subscription.serverRegionId}</strong></div>
   264	                )}
   265	              </div>
   266	            )}
   267	          </div>
   268	        </div>,
   269	        document.body,
   270	      )
   271	    : null;
   272	
   273	  return (
   274	    <nav className="navbar">
   275	      <div className="navbar-left">
   276	        <div className="navbar-brand">
   277	          <Zap size={16} strokeWidth={2.5} />
   278	        </div>
   279	        <span className="navbar-logo-text">OpenNOW</span>
   280	      </div>
   281	
   282	      <div className="navbar-nav">
   283	        {navItems.map((item) => {
   284	          const Icon = item.icon;
   285	          const isActive = currentPage === item.id;
   286	
   287	          return (
   288	            <button
   289	              key={item.id}
   290	              onClick={() => onNavigate(item.id)}
   291	              className={`navbar-link ${isActive ? "active" : ""}`}
   292	            >
   293	              <Icon size={16} />
   294	              <span>{item.label}</span>
   295	            </button>
   296	          );
   297	        })}
   298	      </div>
   299	
   300	      <div className="navbar-right">
   301	        {activeSession && (
   302	          <div className="navbar-session-actions">
   303	            <button
   304	              type="button"
   305	              className={`navbar-session-resume${isResumingSession ? " is-loading" : ""}`}
   306	              title={
   307	                activeSession.serverIp
   308	                  ? activeSessionTitle
   309	                    ? `Resume active cloud session: ${activeSessionTitle}`
   310	                    : "Resume active cloud session"
   311	                  : "Active session found (missing server address)"
   312	              }
   313	              onClick={onResumeSession}
   314	              disabled={isResumingSession || isTerminatingSession || !activeSession.serverIp}
   315	            >
   316	              {isResumingSession ? <Loader2 size={14} className="navbar-session-resume-spin" /> : <PlayCircle size={14} />}
   317	              <span className="navbar-session-resume-text">Resume</span>
   318	              {activeSessionTitle && <span className="navbar-session-resume-game">{activeSessionTitle}</span>}
   319	            </button>
   320	            <button
   321	              type="button"
   322	              className={`navbar-session-terminate${isTerminatingSession ? " is-loading" : ""}`}
   323	              title={
   324	                activeSessionTitle
   325	                  ? `Terminate active cloud session: ${activeSessionTitle}`
   326	                  : "Terminate active cloud session"
   327	              }
   328	              onClick={onTerminateSession}
   329	              disabled={isResumingSession || isTerminatingSession}
   330	            >
   331	              {isTerminatingSession ? <Loader2 size={14} className="navbar-session-resume-spin" /> : <Square size={12} />}
   332	              <span className="navbar-session-terminate-text">Terminate</span>
   333	            </button>
   334	          </div>
   335	        )}
   336	        {(timeLabel || storageLabel) && (
   337	          <div className="navbar-subscription" aria-label="Subscription details">
   338	            {timeLabel && (
   339	              <button
   340	                type="button"
   341	                className={`navbar-subscription-chip navbar-subscription-chip--${timeTone}`}
   342	                title="Show playtime details"
   343	                onClick={() => setModalType("time")}
   344	              >
   345	                <Timer size={14} />
   346	                <span>{timeLabel}</span>
   347	              </button>
   348	            )}
   349	            {storageLabel && (
   350	              <button
   351	                type="button"
   352	                className={`navbar-subscription-chip navbar-subscription-chip--${storageTone}`}
   353	                title="Show storage details"
   354	                onClick={() => setModalType("storage")}
   355	              >
   356	                <HardDrive size={14} />
   357	                <span>{storageLabel}</span>
   358	              </button>
   359	            )}
   360	          </div>
   361	        )}
   362	        {user ? (
   363	          <>
   364	            <div className="navbar-account-container" ref={accountContainerRef}>
   365	              <button
   366	                type="button"
   367	                className="navbar-user navbar-user--clickable"
   368	                onClick={() => setAccountDropdownOpen((previous) => !previous)}
   369	                aria-expanded={accountDropdownOpen}
   370	                aria-haspopup="listbox"
   371	              >
   372	                {user.avatarUrl ? (
   373	                  <img src={user.avatarUrl} alt={user.displayName} className="navbar-avatar" />
   374	                ) : (
   375	                  <div className="navbar-avatar-fallback">
   376	                    <User size={14} />
   377	                  </div>
   378	                )}
   379	                <div className="navbar-user-info">
   380	                  <span className="navbar-username">{user.displayName}</span>
   381	                  {tierInfo && (
   382	                    <span className={`navbar-tier ${tierInfo.className}`}>{tierInfo.label}</span>
   383	                  )}
   384	                </div>
   385	                <ChevronDown
   386	                  size={14}
   387	                  className={`navbar-user-chevron${accountDropdownOpen ? " is-open" : ""}`}
   388	                />
   389	              </button>
   390	              {accountDropdownOpen && (
   391	                <div className="navbar-account-dropdown" role="list" aria-label="Switch account">
   392	                  <div className="navbar-account-dropdown-header">Switch Account</div>
   393	                  <div className="navbar-account-list">
   394	                    {savedAccounts.map((account) => {
   395	                      const accountTierInfo = getTierDisplay(account.membershipTier);
   396	                      const isActive = activeUserId === account.userId;
   397	                      const canRemove = !isActive && savedAccounts.length > 1;
   398	                      return (
   399	                        <div
   400	                          key={account.userId}
   401	                          className={`navbar-account-item${isActive ? " navbar-account-item--active" : ""}`}
   402	                        >
   403	                          <button
   404	                            type="button"
   405	                            className="navbar-account-item-main"
   406	                            onClick={() => {
   407	                              if (!isActive) {
   408	                                onSwitchAccount(account.userId);
   409	                              }
   410	                              setAccountDropdownOpen(false);
   411	                            }}
   412	                            disabled={isActive}
   413	                          >
   414	                            {account.avatarUrl ? (
   415	                              <img
   416	                                src={account.avatarUrl}
   417	                                alt={account.displayName}
   418	                                className="navbar-account-item-avatar"
   419	                              />
   420	                            ) : (
   421	                              <div className="navbar-avatar-fallback navbar-account-item-avatar">
   422	                                <User size={12} />
   423	                              </div>
   424	                            )}
   425	                            <div className="navbar-account-item-info">
   426	                              <span className="navbar-account-item-name">{account.displayName}</span>
   427	                              {account.email && <span className="navbar-account-item-email">{account.email}</span>}
   428	                            </div>
   429	                            <div className="navbar-account-item-right">
   430	                              <span className={`navbar-account-item-tier ${accountTierInfo.className}`}>
   431	                                {accountTierInfo.label}
   432	                              </span>
   433	                              {isActive && (
   434	                                <span className="navbar-account-item-check" aria-label="Active account">
   435	                                  <Check size={14} />
   436	                                </span>
   437	                              )}
   438	                            </div>
   439	                          </button>
   440	                          {canRemove && (
   441	                            <button
   442	                              type="button"
   443	                              className="navbar-account-remove"
   444	                              aria-label={`Remove ${account.displayName}`}
   445	                              onClick={() => {
   446	                                onRemoveAccount(account.userId);
   447	                              }}
   448	                            >
   449	                              <X size={12} />
   450	                            </button>
   451	                          )}
   452	                        </div>
   453	                      );
   454	                    })}
   455	                  </div>
   456	                  <div className="navbar-account-divider" />
   457	                  <button
   458	                    type="button"
   459	                    className="navbar-account-add"
   460	                    onClick={() => {
   461	                      onAddAccount();
   462	                      setAccountDropdownOpen(false);
   463	                    }}
   464	                  >
   465	                    <Plus size={14} />
   466	                    <span>Add account</span>
   467	                  </button>
   468	                  <button
   469	                    type="button"
   470	                    className="navbar-account-signout-all"
   471	                    onClick={() => {
   472	                      onLogout();
   473	                      setAccountDropdownOpen(false);
   474	                    }}
   475	                  >
   476	                    Sign out all accounts
   477	                  </button>
   478	                </div>
   479	              )}
   480	            </div>
   481	          </>
   482	        ) : (
   483	          <div className="navbar-guest">
   484	            <User size={14} />
   485	            <span>Guest</span>
   486	          </div>
   487	        )}
   488	      </div>
   489	      {modal}
   490	    </nav>
   491	  );
   492	}
   493	