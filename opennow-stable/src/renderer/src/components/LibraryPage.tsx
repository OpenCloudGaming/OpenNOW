import { Library, Search, Clock, Gamepad2, Loader2, ArrowUpDown } from "lucide-react";
import type { JSX } from "react";
import type { CatalogSortOption, GameInfo } from "@shared/gfn";
import { GameCard } from "./GameCard";
import { useTranslation } from "../i18n";
import { formatCatalogLastPlayed } from "../utils/lastPlayedFormat";

export interface LibraryPageProps {
  games: GameInfo[];
  searchQuery: string;
  onSearchChange: (query: string) => void;
  onPlayGame: (game: GameInfo) => void;
  isLoading: boolean;
  selectedGameId: string;
  onSelectGame: (id: string) => void;
  selectedVariantByGameId: Record<string, string>;
  onSelectGameVariant: (gameId: string, variantId: string) => void;
  libraryCount: number;
  sortOptions: CatalogSortOption[];
  selectedSortId: string;
  onSortChange: (sortId: string) => void;
}

export function LibraryPage({
  games,
  searchQuery,
  onSearchChange,
  onPlayGame,
  isLoading,
  selectedGameId,
  onSelectGame,
  selectedVariantByGameId,
  onSelectGameVariant,
  libraryCount,
  sortOptions,
  selectedSortId,
  onSortChange,
}: LibraryPageProps): JSX.Element {
  const { t } = useTranslation();
  return (
    <div className="library-page">
      <header className="library-toolbar">
        <div className="library-title">
          <Library className="library-title-icon" size={22} />
          <h1>{t("library.title")}</h1>
        </div>

        <div className="library-search">
          <Search className="library-search-icon" size={16} />
          <input
            type="text"
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            placeholder={t("library.searchPlaceholder")}
            className="library-search-input"
          />
        </div>

        <label className="library-sort">
          <ArrowUpDown size={14} />
          <select value={selectedSortId} onChange={(e) => onSortChange(e.target.value)}>
            {sortOptions.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label}
              </option>
            ))}
          </select>
        </label>

        <span className="library-count">{t("library.gameCount", { count: libraryCount })}</span>
      </header>

      <div className="library-grid-area">
        {isLoading ? (
          <div className="library-empty-state">
            <Loader2 className="library-spinner" size={36} />
            <p>{t("library.empty.loadingLibrary")}</p>
          </div>
        ) : libraryCount === 0 ? (
          <div className="library-empty-state">
            <Gamepad2 className="library-empty-icon" size={44} />
            <h3>{t("library.empty.libraryEmpty")}</h3>
            <p>{t("library.empty.ownedGamesAppearHere")}</p>
          </div>
        ) : games.length === 0 ? (
          <div className="library-empty-state">
            <Search className="library-empty-icon" size={44} />
            <h3>{t("library.empty.noGamesFound")}</h3>
            <p>{t("library.empty.noGamesMatch", { query: searchQuery })}</p>
          </div>
        ) : (
          <div className="game-grid">
            {games.map((game) => (
              <div key={game.id} className="library-game-wrapper">
                <GameCard
                  game={game}
                  isSelected={game.id === selectedGameId}
                  onSelect={() => onSelectGame(game.id)}
                  onPlay={() => onPlayGame(game)}
                  selectedVariantId={selectedVariantByGameId[game.id]}
                  onSelectStore={(variantId) => onSelectGameVariant(game.id, variantId)}
                />
                {game.lastPlayed && (
                  <div className="library-last-played">
                    <Clock size={12} />
                    <span>{formatCatalogLastPlayed(t, game.lastPlayed)}</span>
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
