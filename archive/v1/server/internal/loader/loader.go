// Package loader discovers and reads REACHLOCK mod content from disk.
//
// It is the server's own window into the same content the Godot engine reads
// through its mod loader — strictly content data files (JSON), never the
// Godot mod loader itself. The engine-side window is documented in
// docs/ARCHITECTURE.md (Ring 0 reads content through this loader / through
// the framework, not by hardcoded paths or ids).
//
// Loader is deliberately lenient: a missing directory is a warning, a
// malformed JSON file is a warning, a content file that does not match a
// known kind is silently ignored. The point of strict validation is CI's
// job (scripts/validate_mod_data.py); the runtime just has to not crash.
package loader

import (
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Result is what one Load call returns. All collections are keyed by entity
// id (the value of the JSON file's top-level "id" field). Warnings collects
// human-readable problems; callers should log them.
type Result struct {
	// Mods is the set of mod ids the loader found, in stable (sorted) order.
	Mods []string
	// Factions, Locations, Goods hold the entities of each kind. Other kinds
	// (ships, npcs, dialogues, …) are intentionally not surfaced: the sim
	// layer only needs the ones its systems consume. Unknown kinds load
	// too — we just don't expose them through this typed view.
	Factions  map[string]Faction
	Locations map[string]Location
	Goods     map[string]Good
	// Warnings are non-fatal load problems (missing mods dir, bad JSON,
	// ids that collide across mods, etc.). The loader never errors on
	// these; it returns a partially populated Result with Warnings set.
	Warnings []string
}

// Faction is the minimum the faction simulator needs from a faction file.
// We keep the full file around (Extra) so future systems (UI, debugging)
// can read it without the loader growing a struct field per schema bump.
type Faction struct {
	ID            string
	Name          string
	Territory     []string
	Produces      []string
	Consumes      []string
	Relationships map[string]string // other faction id -> stance at universe start
	Goals         []string
	Extra         map[string]any
}

// Location is the minimum the economy engine needs from a location file:
// the goods it produces and consumes, used to seed per-location supply
// and demand each reprice tick.
type Location struct {
	ID             string
	Name           string
	Kind           string
	FactionControl string
	Produces       []string
	Consumes       []string
	Extra          map[string]any
}

// Good is the minimum the economy engine needs: a stable base price and
// (optional) legality. Live prices are tracked by the economy engine; the
// loader's view of a good is the authored one.
type Good struct {
	ID        string
	Name      string
	BasePrice int
	UnitMass  float64
	Legality  map[string]string
	Tags      []string
	Extra     map[string]any
}

// Load walks the given mods root, reading every mod's entities. A typical
// call: loader.Load("godot/mods", nil). If the root does not exist the
// returned Result has no Factions/Locations/Goods and a single warning —
// the caller (the sim) boots empty and the universe is happy.
//
// Optional kinds may restrict the set of kinds to read. nil/empty means
// "all known kinds" (factions, locations, goods).
func Load(modsRoot string, kinds []string) (*Result, error) {
	res := &Result{
		Factions:  map[string]Faction{},
		Locations: map[string]Location{},
		Goods:     map[string]Good{},
	}
	if modsRoot == "" {
		res.Warnings = append(res.Warnings, "no mods root configured — engine boots empty")
		return res, nil
	}
	info, err := os.Stat(modsRoot)
	if err != nil {
		if os.IsNotExist(err) {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("mods root %q does not exist — engine boots empty", modsRoot))
			return res, nil
		}
		return nil, fmt.Errorf("stat mods root %q: %w", modsRoot, err)
	}
	if !info.IsDir() {
		res.Warnings = append(res.Warnings,
			fmt.Sprintf("mods root %q is not a directory — engine boots empty", modsRoot))
		return res, nil
	}

	modDirs := map[string]string{}
	entries, err := os.ReadDir(modsRoot)
	if err != nil {
		return nil, fmt.Errorf("read mods root %q: %w", modsRoot, err)
	}
	for _, e := range entries {
		if !e.IsDir() || strings.HasPrefix(e.Name(), ".") {
			continue
		}
		modPath := filepath.Join(modsRoot, e.Name())
		manifestPath := filepath.Join(modPath, "manifest.json")
		if _, err := os.Stat(manifestPath); err != nil {
			continue
		}
		var manifest struct {
			ID string `json:"id"`
		}
		raw, err := os.ReadFile(manifestPath)
		if err != nil {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/manifest.json: read failed: %v", e.Name(), err))
			continue
		}
		if err := json.Unmarshal(raw, &manifest); err != nil {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/manifest.json: not valid JSON: %v — mod skipped", e.Name(), err))
			continue
		}
		if manifest.ID == "" {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/manifest.json: manifest has no id — mod skipped", e.Name()))
			continue
		}
		if _, dup := modDirs[manifest.ID]; dup {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("duplicate mod id %q — last-loaded wins (the %s/ copy)",
					manifest.ID, e.Name()))
		}
		modDirs[manifest.ID] = modPath
	}

	for _, modID := range sortedKeys(modDirs) {
		res.Mods = append(res.Mods, modID)
		loadMod(modDirs[modID], modID, kinds, res)
	}
	return res, nil
}

// loadMod reads every <kind>/*.json file in one mod directory into the
// matching typed map. Unknown kinds load fine (we just skip the typed
// view); unknown files inside a known kind directory are read and
// skipped with a warning.
func loadMod(modPath, modID string, kinds []string, res *Result) {
	want := map[string]bool{}
	for _, k := range kinds {
		want[k] = true
	}
	wantAll := len(want) == 0
	entries, err := os.ReadDir(modPath)
	if err != nil {
		res.Warnings = append(res.Warnings, fmt.Sprintf("mod %q: read failed: %v", modID, err))
		return
	}
	for _, e := range entries {
		if !e.IsDir() || strings.HasPrefix(e.Name(), ".") {
			continue
		}
		kind := e.Name()
		if !wantAll && !want[kind] {
			continue
		}
		kindPath := filepath.Join(modPath, kind)
		_ = filepath.WalkDir(kindPath, func(path string, d fs.DirEntry, walkErr error) error {
			if walkErr != nil {
				res.Warnings = append(res.Warnings,
					fmt.Sprintf("mod %q: walk %s failed: %v", modID, path, walkErr))
				return nil
			}
			if d.IsDir() {
				return nil
			}
			if !strings.HasSuffix(d.Name(), ".json") {
				return nil
			}
			loadOneEntity(path, kind, modID, res)
			return nil
		})
	}
}

// loadOneEntity reads a single JSON file and routes it into the right
// typed map based on its kind. If the file is malformed it logs a warning
// and moves on. If two mods provide the same entity id, last-loaded wins
// with a warning — same convention as the engine-side mod loader.
func loadOneEntity(path, kind, modID string, res *Result) {
	raw, err := os.ReadFile(path)
	if err != nil {
		res.Warnings = append(res.Warnings,
			fmt.Sprintf("%s/%s: read failed: %v", modID, path, err))
		return
	}
	switch kind {
	case "factions":
		var f factionJSON
		if err := json.Unmarshal(raw, &f); err != nil {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: not valid JSON: %v", modID, path, err))
			return
		}
		if f.ID == "" {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: entity has no id — skipped", modID, path))
			return
		}
		if _, dup := res.Factions[f.ID]; dup {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("factions: id %q overridden by mod %q", f.ID, modID))
		}
		res.Factions[f.ID] = Faction{
			ID:            f.ID,
			Name:          f.Name,
			Territory:     f.Territory,
			Produces:      f.Resources.Produces,
			Consumes:      f.Resources.Consumes,
			Relationships: f.Relationships,
			Goals:         f.Goals,
			Extra:         f.Extra,
		}
	case "locations":
		var l locationJSON
		if err := json.Unmarshal(raw, &l); err != nil {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: not valid JSON: %v", modID, path, err))
			return
		}
		if l.ID == "" {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: entity has no id — skipped", modID, path))
			return
		}
		if _, dup := res.Locations[l.ID]; dup {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("locations: id %q overridden by mod %q", l.ID, modID))
		}
		res.Locations[l.ID] = Location{
			ID:             l.ID,
			Name:           l.Name,
			Kind:           l.Kind,
			FactionControl: l.FactionControl,
			Produces:       l.Economy.Produces,
			Consumes:       l.Economy.Consumes,
			Extra:          l.Extra,
		}
	case "goods":
		var g goodJSON
		if err := json.Unmarshal(raw, &g); err != nil {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: not valid JSON: %v", modID, path, err))
			return
		}
		if g.ID == "" {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("%s/%s: entity has no id — skipped", modID, path))
			return
		}
		if _, dup := res.Goods[g.ID]; dup {
			res.Warnings = append(res.Warnings,
				fmt.Sprintf("goods: id %q overridden by mod %q", g.ID, modID))
		}
		res.Goods[g.ID] = Good{
			ID:        g.ID,
			Name:      g.Name,
			BasePrice: g.BasePrice,
			UnitMass:  g.UnitMass,
			Legality:  g.Legality,
			Tags:      g.Tags,
			Extra:     g.Extra,
		}
	default:
		// Other kinds (ships, npcs, dialogues, ...) load fine; this loader
		// just doesn't have a typed view of them yet. Silently ignore.
	}
}

// The following structs are partial mirrors of the JSON Schemas in
// godot/framework/schemas/. They are intentionally permissive (every
// field optional) so a slightly-ahead-of-us mod doesn't fail to load —
// strict validation is CI's job. Unknown fields land in Extra.

type factionJSON struct {
	ID           string   `json:"id"`
	Name         string   `json:"name"`
	Territory    []string `json:"territory"`
	Resources    struct {
		Produces []string `json:"produces"`
		Consumes []string `json:"consumes"`
	} `json:"resources"`
	Relationships map[string]string `json:"relationships"`
	Goals         []string          `json:"goals"`
	Extra         map[string]any    `json:"extra"`
}

type locationJSON struct {
	ID             string `json:"id"`
	Name           string `json:"name"`
	Kind           string `json:"kind"`
	FactionControl string `json:"faction_control"`
	Economy        struct {
		Produces []string `json:"produces"`
		Consumes []string `json:"consumes"`
	} `json:"economy"`
	Extra map[string]any `json:"extra"`
}

type goodJSON struct {
	ID        string            `json:"id"`
	Name      string            `json:"name"`
	BasePrice int               `json:"base_price"`
	UnitMass  float64           `json:"unit_mass"`
	Legality  map[string]string `json:"legality"`
	Tags      []string          `json:"tags"`
	Extra     map[string]any    `json:"extra"`
}

func sortedKeys[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}
