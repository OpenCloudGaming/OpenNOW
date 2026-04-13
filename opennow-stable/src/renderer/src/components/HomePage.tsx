import { Search, LayoutGrid, Library, ArrowUpDown, Filter, Loader2, Sparkles } from "lucide-react";
import type { JSX } from "react";
import type { CatalogFilterGroup, CatalogSortOption, GameInfo } from "@shared/gfn";
import { GameCard } from "./GameCard";

export interface HomePageProps {
  games: GameInfo[];
  source: "main" | "library";
  onSourceChange: (source: "main" | "library") => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  isLoading: boolean;
  selectedGameId: string;
  onSelectGame: (id: string) => void;
  selectedVariantByGameId: Record<string, string>;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  filterGroups: CatalogFilterGroup[];
  selectedFilterIds: string[];
  onToggleFilter: (filterId: string) => void;
  sortOptions: CatalogSortOption[];
  selectedSortId: string;
  onSortChange: (sortId: string) => void;
  totalCount: number;
  supportedCount: number;
}

export function HomePage({
  games,
  source,
  onSourceChange,
  searchQuery,
  onSearchChange,
  onPlayGame,
  isLoading,
  selectedGameId,
  onSelectGame,
  selectedVariantByGameId,
  onSelectGameVariant,
  filterGroups,
  selectedFilterIds,
  onToggleFilter,
  sortOptions,
  selectedSortId,
  onSortChange,
  totalCount,
  supportedCount,
}: HomePageProps): JSX.Element {
  const hasGames = games.length > 0;
  const quickGroups = filterGroups.filter((group) => ["digital_store", "genre", "subscriptions"].includes(group.id));
  const primaryOptions = quickGroups.flatMap((group) => group.options.slice(0, group.id === "genre" ? 8 : group.options.length));

  return (
    <div className="home-page home-page--catalog">
      <header className="catalog-hero">
        <div className="catalog-hero-copy">
          <span className="catalog-kicker">
            <Sparkles size={14} />
            Cloud catalog
          </span>
          <div className="catalog-hero-title-row">
            <h1>Discover your next session</h1>
            <div className="home-tabs">
              <button
                className={`home-tab ${source === "main" ? "active" : ""}`}
                onClick={() => onSourceChange("main")}
                disabled={isLoading}
              >
                <LayoutGrid size={15} />
                Catalog
              </button>
              <button
                className={`home-tab ${source === "library" ? "active" : ""}`}
                onClick={() => onSourceChange("library")}
                disabled={isLoading}
              >
                <Library size={15} />
                Library
              </button>
            </div>
          </div>
          <p>Official GFN search, server-defined filters, and OpenNOW card actions.</p>
        </div>
        <div className="catalog-hero-stats">
          <div className="catalog-stat-card">
            <span className="catalog-stat-label">Visible</span>
            <strong>{games.length}</strong>
          </div>
          <div className="catalog-stat-card">
            <span className="catalog-stat-label">Supported</span>
            <strong>{supportedCount || games.length}</strong>
          </div>
          <div className="catalog-stat-card">
            <span className="catalog-stat-label">Total matches</span>
            <strong>{totalCount || games.length}</strong>
          </div>
        </div>
      </header>

      <section className="catalog-controls-panel">
        <div className="catalog-controls-toprow">
          <div className="home-search catalog-search">
            <Search className="home-search-icon" size={16} />
            <input
              type="text"
              className="home-search-input"
              placeholder="Search titles, stores, genres, and features..."
              value={searchQuery}
              onChange={(e) => onSearchChange(e.target.value)}
            />
          </div>

          <label className="catalog-sort-control">
            <ArrowUpDown size={15} />
            <span>Sort</span>
            <select value={selectedSortId} onChange={(e) => onSortChange(e.target.value)} disabled={isLoading}>
              {sortOptions.map((option) => (
                <option key={option.id} value={option.id}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        <div className="catalog-filter-groups">
          {quickGroups.map((group) => (
            <div key={group.id} className="catalog-filter-group">
              <div className="catalog-filter-group-label">
                <Filter size={13} />
                {group.label}
              </div>
              <div className="catalog-filter-chip-row">
                {group.options.slice(0, group.id === "genre" ? 8 : group.options.length).map((option) => {
                  const active = selectedFilterIds.includes(option.id);
                  return (
                    <button
                      key={option.id}
                      type="button"
                      className={`catalog-filter-chip ${active ? "active" : ""}`}
                      onClick={() => onToggleFilter(option.id)}
                    >
                      {option.label}
                    </button>
                  );
                })}
              </div>
            </div>
          ))}
          {quickGroups.length === 0 && primaryOptions.length === 0 && (
            <span className="catalog-filters-empty">Filters unavailable</span>
          )}
        </div>
      </section>

      <div className="catalog-results-header">
        <div>
          <h2>{searchQuery.trim() ? `Results for “${searchQuery.trim()}”` : "Browse catalog"}</h2>
          <p>{selectedFilterIds.length > 0 ? `${selectedFilterIds.length} active filter${selectedFilterIds.length === 1 ? "" : "s"}` : "All supported cloud games"}</p>
        </div>
        <span className="home-count">
          {isLoading ? "Refreshing…" : `${games.length} shown`}
        </span>
      </div>

      <div className="home-grid-area">
        {isLoading ? (
          <div className="home-empty-state">
            <Loader2 className="home-spinner" size={36} />
            <p>Refreshing catalog…</p>
          </div>
        ) : !hasGames ? (
          <div className="home-empty-state">
            <LayoutGrid size={44} className="home-empty-icon" />
            <h3>No games found</h3>
            <p>{searchQuery || selectedFilterIds.length > 0 ? "Try another search or adjust filters" : "No supported games are currently available"}</p>
          </div>
        ) : (
          <div className="game-grid">
            {games.map((game) => (
              <GameCard
                key={game.id}
                game={game}
                isSelected={game.id === selectedGameId}
                onSelect={() => onSelectGame(game.id)}
                onPlay={() => onPlayGame(game)}
                selectedVariantId={selectedVariantByGameId[game.id]}
                onSelectStore={(variantId) => onSelectGameVariant(game.id, variantId)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
