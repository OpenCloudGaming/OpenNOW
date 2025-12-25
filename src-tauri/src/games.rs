use serde::{Deserialize, Serialize};
use crate::api::{Game, StoreType};

/// User's game library
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameLibrary {
    pub owned_games: Vec<LibraryGame>,
    pub favorites: Vec<String>, // Game IDs
    pub recently_played: Vec<RecentGame>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryGame {
    pub game: Game,
    pub is_favorite: bool,
    pub last_played: Option<chrono::DateTime<chrono::Utc>>,
    pub total_playtime_minutes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentGame {
    pub game_id: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub last_played: chrono::DateTime<chrono::Utc>,
    pub playtime_minutes: u64,
}

/// Game categories for browsing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameCategory {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub games: Vec<Game>,
}

/// Featured games section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturedSection {
    pub section_type: FeaturedType,
    pub title: String,
    pub games: Vec<Game>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeaturedType {
    Hero,           // Large hero banner
    Featured,       // Featured games row
    NewReleases,    // Newly added games
    Popular,        // Popular games
    FreeToPlay,     // Free-to-play games
    OptimizedFor,   // Games optimized for GFN
    Category(String),
}

/// Store connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConnection {
    pub store_type: StoreType,
    pub is_connected: bool,
    pub username: Option<String>,
    pub game_count: Option<u32>,
}

/// Game launch options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchOptions {
    pub game_id: String,
    pub store_type: StoreType,
    pub store_id: String,
    /// Preferred server region
    pub region: Option<String>,
    /// Custom launch parameters
    pub launch_params: Option<String>,
}

impl GameLibrary {
    pub fn new() -> Self {
        Self {
            owned_games: vec![],
            favorites: vec![],
            recently_played: vec![],
        }
    }

    pub fn add_favorite(&mut self, game_id: String) {
        if !self.favorites.contains(&game_id) {
            self.favorites.push(game_id);
        }
    }

    pub fn remove_favorite(&mut self, game_id: &str) {
        self.favorites.retain(|id| id != game_id);
    }

    pub fn add_recent(&mut self, game: RecentGame) {
        // Remove existing entry for this game
        self.recently_played.retain(|g| g.game_id != game.game_id);

        // Add to front
        self.recently_played.insert(0, game);

        // Keep only last 20 games
        self.recently_played.truncate(20);
    }
}

impl Default for GameLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// Deep link URL schemes for launching games
pub mod deep_link {
    use super::StoreType;

    /// Generate a deep link URL for launching a game
    pub fn generate_launch_url(store_type: &StoreType, store_id: &str) -> String {
        match store_type {
            StoreType::Steam => format!("steam://run/{}", store_id),
            StoreType::Epic => format!("com.epicgames.launcher://apps/{}?action=launch", store_id),
            StoreType::Ubisoft => format!("uplay://launch/{}", store_id),
            StoreType::Origin => format!("origin://launchgame/{}", store_id),
            StoreType::GoG => format!("goggalaxy://openGameView/{}", store_id),
            StoreType::Xbox => format!("msxbox://game/?productId={}", store_id),
            StoreType::EaApp => format!("origin://launchgame/{}", store_id),
            StoreType::Other(_) => format!("gfn://launch/{}", store_id),
        }
    }

    /// GFN-specific deep link for direct game launch
    pub fn generate_gfn_launch_url(game_id: &str, store_type: &StoreType) -> String {
        let store_param = match store_type {
            StoreType::Steam => "STEAM",
            StoreType::Epic => "EPIC",
            StoreType::Ubisoft => "UBISOFT",
            StoreType::Origin => "ORIGIN",
            StoreType::GoG => "GOG",
            StoreType::Xbox => "XBOX",
            StoreType::EaApp => "EA_APP",
            StoreType::Other(name) => name,
        };

        format!(
            "geforcenow://game/?game_id={}&store_type={}",
            game_id, store_param
        )
    }
}
