import { Search, LayoutGrid, Globe, Loader2 } from "lucide-react";
import type { JSX } from "react";
import type { GameInfo } from "@shared/gfn";
import { GameCard } from "./GameCard";

export interface HomePageProps {
  games: GameInfo[];
  source: "main" | "library" | "public";
  onSourceChange: (source: "main" | "library" | "public") => void;
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  isLoading: boolean;
  selectedGameId: string;
  onSelectGame: (id: string) => void;
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
}: HomePageProps): JSX.Element {
  const hasGames = games.length > 0;

  return (
    <div className="home-page">
      {/* Top bar: tabs + search + count */}
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
          <button
            className={`home-tab ${source === "public" ? "active" : ""}`}
            onClick={() => onSourceChange("public")}
            disabled={isLoading}
          >
            <Globe size={15} />
            Public
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

        <span className="home-count">
          {isLoading ? "Loading..." : `${games.length} game${games.length !== 1 ? "s" : ""}`}
        </span>
      </header>

      {/* Game grid */}
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
              {searchQuery
                ? "Try adjusting your search terms"
                : "Check back later for new additions"}
            </p>
          </div>
        ) : (
          <div className="game-grid">
            {games.map((game, index) => (
              <GameCard
                key={`${game.id}-${index}`}
                game={game}
                isSelected={game.id === selectedGameId}
                onSelect={() => onSelectGame(game.id)}
                onPlay={() => onPlayGame(game)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
