import type { AuthUser } from "@shared/gfn";
import { House, Library, Settings, User, LogOut, Zap } from "lucide-react";

interface NavbarProps {
  currentPage: "home" | "library" | "settings";
  onNavigate: (page: "home" | "library" | "settings") => void;
  user: AuthUser | null;
  onLogout: () => void;
}

function getTierDisplay(tier: string): { label: string; className: string } {
  const t = tier.toUpperCase();
  if (t === "ULTIMATE") return { label: "Ultimate", className: "tier-ultimate" };
  if (t === "PRIORITY" || t === "PERFORMANCE") return { label: "Priority", className: "tier-priority" };
  return { label: "Free", className: "tier-free" };
}

export function Navbar({ currentPage, onNavigate, user, onLogout }: NavbarProps): JSX.Element {
  const navItems = [
    { id: "home" as const, label: "Store", icon: House },
    { id: "library" as const, label: "Library", icon: Library },
    { id: "settings" as const, label: "Settings", icon: Settings },
  ];

  const tierInfo = user ? getTierDisplay(user.membershipTier) : null;

  return (
    <nav className="navbar">
      <div className="navbar-left">
        <div className="navbar-brand">
          <Zap size={16} strokeWidth={2.5} />
        </div>
        <span className="navbar-logo-text">OpenNOW</span>
      </div>

      <div className="navbar-nav">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentPage === item.id;

          return (
            <button
              key={item.id}
              onClick={() => onNavigate(item.id)}
              className={`navbar-link ${isActive ? "active" : ""}`}
            >
              <Icon size={16} />
              <span>{item.label}</span>
            </button>
          );
        })}
      </div>

      <div className="navbar-right">
        {user ? (
          <>
            <div className="navbar-user">
              {user.avatarUrl ? (
                <img src={user.avatarUrl} alt={user.displayName} className="navbar-avatar" />
              ) : (
                <div className="navbar-avatar-fallback">
                  <User size={14} />
                </div>
              )}
              <div className="navbar-user-info">
                <span className="navbar-username">{user.displayName}</span>
                {tierInfo && (
                  <span className={`navbar-tier ${tierInfo.className}`}>{tierInfo.label}</span>
                )}
              </div>
            </div>
            <button onClick={onLogout} className="navbar-logout" title="Sign out">
              <LogOut size={16} />
            </button>
          </>
        ) : (
          <div className="navbar-guest">
            <User size={14} />
            <span>Guest</span>
          </div>
        )}
      </div>
    </nav>
  );
}
