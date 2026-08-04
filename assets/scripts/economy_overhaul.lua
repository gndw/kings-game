-- economy_overhaul.lua
-- Example script mod for Kings Game.
-- This script receives callback events from the game:
--   - on_day:   fired every simulated day
--   - on_month: fired when the in-game month rolls over
--
-- Scripts access the ECS world via the `world` global provided by BMS.
-- Component data is accessible through Bevy's reflection system.
--
-- This is a cold-path hook: it fires once per day/month, NOT per entity.
-- For per-entity hot-loop computation, use Rust systems or modifier tables.

-- Print a message on load
print("[economy_overhaul] script loaded")

-- Monthly hook: log a chronicle entry every month
function on_month()
    print("[economy_overhaul] a new month begins")
end

-- Daily hook: runs once per simulated day
function on_day()
    -- Intentionally minimal — daily hooks at scale can be expensive.
    -- Keep logic here O(1) or very lightweight.
end
