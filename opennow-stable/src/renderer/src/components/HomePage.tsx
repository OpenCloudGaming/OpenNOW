import { Search, LayoutGrid, Loader2, ArrowUpDown, Filter, ChevronDown } from "lucide-react";
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
  const visibleFilterGroups = filterGroups.filter((group) => ["digital_store", "genre", "subscriptions"].includes(group.id));
  const activeFilterCount = selectedFilterIds.length;

  return (
    <div className="home-page">
      <header className="home-toolbar">
        <div className="home-tabs">
          <button
            className={`home-tab ${source === "main" ? "active" : ""}`}
            onClick={() => onSourceChange("main")}
            disabled={isLoading}
          >
            <LayoutGrid size={15} />
            Catalog
          </button>
        </div>

        <div className="home-search">
          <Search className="home-search-icon" size={16} />
          <input
            type="text"
            className="home-search-input"
            placeholder="Search games..."
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
          />
        </div>

        {visibleFilterGroups.length > 0 && (
          <details className="home-filter-dropdown">
            <summary className="home-filter-dropdown-trigger">
              <span className="home-filter-dropdown-label">
                <Filter size={14} />
                Filters
              </span>
              {activeFilterCount > 0 && <span className="home-filter-dropdown-count">{activeFilterCount}</span>}
              <ChevronDown size={14} className="home-filter-dropdown-chevron" />
            </summary>
            <div className="home-filter-dropdown-menu">
              {visibleFilterGroups.map((group) => (
                <div key={group.id} className="home-filter-dropdown-group">
                  <div className="home-filter-group-label">{group.label}</div>
                  <div className="home-filter-chips">
                    {group.options.slice(0, group.id === "genre" ? 8 : group.options.length).map((option) => {
                      const active = selectedFilterIds.includes(option.id);
                      return (
                        <button
                          key={option.id}
                          type="button"
                          className={`home-filter-chip ${active ? "active" : ""}`}
                          onClick={() => onToggleFilter(option.id)}
                        >
                          {option.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          </details>
        )}

        <label className="home-sort">
          <ArrowUpDown size={14} />
          <select value={selectedSortId} onChange={(e) => onSortChange(e.target.value)} disabled={isLoading}>
            {sortOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <span className="home-count">
          {isLoading
            ? "Loading..."
            : `${games.length} shown${totalCount > games.length ? ` · ${totalCount} total` : ""}${supportedCount > 0 ? ` · ${supportedCount} supported` : ""}`}
        </span>
      </header>

      <div className="home-grid-area">
        {isLoading ? (
          <div className="home-empty-state">
            <Loader2 className="home-spinner" size={36} />
            <p>Loading games...</p>
          </div>
        ) : !hasGames ? (
          <div className="home-empty-state">
            <LayoutGrid size={44} className="home-empty-icon" />
            <h3>No games found</h3>
            <p>
              {searchQuery || selectedFilterIds.length > 0
                ? "Try adjusting your search terms or filters"
                : "Check back later for new additions"}
            </p>
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
