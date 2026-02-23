function scrollToId(id) {
    try {
        document.getElementById(id).scrollIntoView({ behavior: "smooth", block: "center" });
    } catch(e) {
        
    }
}

function attachCarousel(id) {
    if (document.documentElement.clientWidth > 400) {
        return bulmaCarousel.attach(id, {
            slidesToScroll: 3,
            slidesToShow: 3,
            pagination: false,
            infinite: false,
            loop: true,
        });
    } else {
        return bulmaCarousel.attach(id, {
            slidesToScroll: 1,
            slidesToShow: 1,
            pagination: false,
            loop: true,
            infinite: false,
        });
        
    }
}

// ============================================================
//   THEME MANAGEMENT
// ============================================================

const THEMES = [
    "dark",
    "light",
    "solarized",
    "dracula",
    "nord",
    "rose-pine",
    "ocean",
    "gruvbox",
    "catppuccin",
    "high-contrast",
];
const THEME_STORAGE_KEY = "rsplayer-theme";

// bg, primary-text, accent, ui-elements colours used for the swatch preview
const THEME_META = {
    "dark":          { label: "Dark",          bg: "#121212", text: "#FFFFFF", accent: "#1DB954", ui: "#282828" },
    "light":         { label: "Light",         bg: "#f5f5f5", text: "#1a1a1a", accent: "#1a8f3c", ui: "#e0e0e0" },
    "solarized":     { label: "Solarized",     bg: "#002b36", text: "#eee8d5", accent: "#2aa198", ui: "#073642" },
    "dracula":       { label: "Dracula",       bg: "#282a36", text: "#f8f8f2", accent: "#bd93f9", ui: "#44475a" },
    "nord":          { label: "Nord",          bg: "#2e3440", text: "#eceff4", accent: "#88c0d0", ui: "#3b4252" },
    "rose-pine":     { label: "Rose Pine",     bg: "#191724", text: "#e0def4", accent: "#eb6f92", ui: "#26233a" },
    "ocean":         { label: "Ocean",         bg: "#0f1923", text: "#cdd6f4", accent: "#4fc3f7", ui: "#1a2a3a" },
    "gruvbox":       { label: "Gruvbox",       bg: "#282828", text: "#ebdbb2", accent: "#b8bb26", ui: "#3c3836" },
    "catppuccin":    { label: "Catppuccin",    bg: "#1e1e2e", text: "#cdd6f4", accent: "#cba6f7", ui: "#313244" },
    "high-contrast": { label: "Hi-Contrast",  bg: "#000000", text: "#ffffff", accent: "#00ff00", ui: "#1a1a1a" },
};

/**
 * Apply a theme by setting data-theme on <html> and persisting to localStorage.
 * Returns the theme name that was applied.
 */
function applyTheme(theme) {
    if (!THEMES.includes(theme)) {
        theme = "dark";
    }
    document.documentElement.setAttribute("data-theme", theme);
    try {
        localStorage.setItem(THEME_STORAGE_KEY, theme);
    } catch(e) {}
    return theme;
}

/**
 * Return the currently active theme name.
 */
function getTheme() {
    try {
        const stored = localStorage.getItem(THEME_STORAGE_KEY);
        if (stored && THEMES.includes(stored)) {
            return stored;
        }
    } catch(e) {}
    // Fallback: respect prefers-color-scheme
    if (window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches) {
        return "light";
    }
    return "dark";
}

/**
 * Advance to the next theme in the cycle and apply it.
 * Returns the new theme name.
 */
function cycleTheme() {
    const current = getTheme();
    const idx = THEMES.indexOf(current);
    const next = THEMES[(idx + 1) % THEMES.length];
    return applyTheme(next);
}

/**
 * Return a JSON string with all theme metadata (for the settings theme picker).
 * Keys: label, bg, text, accent, ui
 */
function getThemeMeta(theme) {
    const meta = THEME_META[theme] || THEME_META["dark"];
    return JSON.stringify(meta);
}

/**
 * Return a JSON string array of all available theme names.
 */
function getAllThemes() {
    return JSON.stringify(THEMES);
}

/**
 * Return a JSON string of the full THEME_META map.
 */
function getAllThemeMeta() {
    return JSON.stringify(THEME_META);
}

// Apply saved (or system-preferred) theme as early as possible to avoid flash.
(function() {
    applyTheme(getTheme());
}());

// ============================================================
//   QUEUE DRAG-AND-DROP AUTO-SCROLL
// ============================================================

let _queueScrollRAF = null;

function attachQueueDragScroll() {
    const list = document.querySelector('.scroll-list');
    if (!list) return;

    // Avoid attaching multiple start listeners
    if (list._hasDragScroll) return;
    list._hasDragScroll = true;

    const EDGE = 150;     // px from top/bottom to start scrolling
    const BASE_SPEED = 20;

    const onDragOver = function(e) {
        e.preventDefault(); // Necessary to allow dropping and continuous events

        const rect = list.getBoundingClientRect();
        const y = e.clientY;
        
        const distTop = y - rect.top;
        const distBot = rect.bottom - y;

        let speed = 0;

        // Scroll Up: cursor near top or above
        if (distTop < EDGE) {
            // Intensity increases as we go higher
            // At EDGE: 0. At 0: 1. At -100: 2.
            let intensity = (EDGE - distTop) / EDGE;
            speed = -BASE_SPEED * Math.pow(Math.max(0, intensity), 1.5);
        } 
        // Scroll Down: cursor near bottom or below
        else if (distBot < EDGE) {
            let intensity = (EDGE - distBot) / EDGE;
            speed = BASE_SPEED * Math.pow(Math.max(0, intensity), 1.5);
        }

        if (_queueScrollRAF) cancelAnimationFrame(_queueScrollRAF);
        
        if (Math.abs(speed) > 1) {
            const scroll = () => {
                list.scrollTop += speed;
                _queueScrollRAF = requestAnimationFrame(scroll);
            };
            _queueScrollRAF = requestAnimationFrame(scroll);
        }
    };

    const cleanup = function() {
        if (_queueScrollRAF) {
            cancelAnimationFrame(_queueScrollRAF);
            _queueScrollRAF = null;
        }
        window.removeEventListener('dragover', onDragOver);
        window.removeEventListener('dragend', cleanup);
        window.removeEventListener('drop', cleanup);
    };

    // Only activate global scroll behavior when a drag starts FROM the list
    list.addEventListener('dragstart', function() {
        window.addEventListener('dragover', onDragOver, {passive: false});
        window.addEventListener('dragend', cleanup);
        window.addEventListener('drop', cleanup);
    });
}

