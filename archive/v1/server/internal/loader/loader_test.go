package loader

import (
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"testing"
)

// TestLoad_NonexistentRoot is the "engine boots empty" path the
// brief calls out. A missing mods root must not error; it must
// return a partial Result with a single warning.
func TestLoad_NonexistentRoot(t *testing.T) {
	res, err := Load("/no/such/path/should/exist", nil)
	if err != nil {
		t.Fatalf("missing root should not error, got %v", err)
	}
	if len(res.Warnings) != 1 {
		t.Errorf("missing root should produce 1 warning, got %d: %v", len(res.Warnings), res.Warnings)
	}
	if len(res.Factions) != 0 || len(res.Locations) != 0 || len(res.Goods) != 0 {
		t.Errorf("missing root should produce empty collections, got %+v", res)
	}
}

// TestLoad_EmptyRoot: an existing but empty mods root is a valid
// "no mods installed" state. The sim can boot with zero factions.
func TestLoad_EmptyRoot(t *testing.T) {
	tmp := t.TempDir()
	res, err := Load(tmp, nil)
	if err != nil {
		t.Fatalf("empty root should not error, got %v", err)
	}
	if len(res.Mods) != 0 {
		t.Errorf("empty root should have zero mods, got %v", res.Mods)
	}
	if len(res.Warnings) != 0 {
		t.Errorf("empty root should have zero warnings, got %v", res.Warnings)
	}
}

// TestLoad_SyntheticMod builds a tiny mod on disk and asserts the
// loader reads factions, locations, and goods from it. The faction
// data is shaped like real REACHLOCK content, so the test doubles
// as documentation: this is what a real load looks like.
func TestLoad_SyntheticMod(t *testing.T) {
	tmp := t.TempDir()
	modDir := filepath.Join(tmp, "synthetic")
	for _, sub := range []string{"factions", "locations", "goods"} {
		if err := os.MkdirAll(filepath.Join(modDir, sub), 0o755); err != nil {
			t.Fatal(err)
		}
	}
	manifest := `{"id": "synthetic", "name": "Synthetic", "version": "0.0.1", "provides": {}}`
	if err := os.WriteFile(filepath.Join(modDir, "manifest.json"), []byte(manifest), 0o644); err != nil {
		t.Fatal(err)
	}
	faction := `{"id": "test_faction", "name": "Test Faction", "goals": ["expand"], "resources": {"produces": ["ore"]}}`
	if err := os.WriteFile(filepath.Join(modDir, "factions", "test_faction.json"), []byte(faction), 0o644); err != nil {
		t.Fatal(err)
	}
	location := `{"id": "test_station", "name": "Test Station", "kind": "station", "economy": {"produces": ["food"], "consumes": ["ore"]}}`
	if err := os.WriteFile(filepath.Join(modDir, "locations", "test_station.json"), []byte(location), 0o644); err != nil {
		t.Fatal(err)
	}
	good := `{"id": "test_good", "name": "Test Good", "base_price": 42}`
	if err := os.WriteFile(filepath.Join(modDir, "goods", "test_good.json"), []byte(good), 0o644); err != nil {
		t.Fatal(err)
	}

	res, err := Load(tmp, nil)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if got, want := res.Mods, []string{"synthetic"}; !reflect.DeepEqual(got, want) {
		t.Errorf("Mods = %v, want %v", got, want)
	}
	if _, ok := res.Factions["test_faction"]; !ok {
		t.Errorf("test_faction not loaded; have %v", keysOf(res.Factions))
	}
	if got, want := res.Factions["test_faction"].Produces, []string{"ore"}; !reflect.DeepEqual(got, want) {
		t.Errorf("test_faction.Produces = %v, want %v", got, want)
	}
	if _, ok := res.Locations["test_station"]; !ok {
		t.Errorf("test_station not loaded; have %v", keysOf(res.Locations))
	}
	if got, want := res.Locations["test_station"].Consumes, []string{"ore"}; !reflect.DeepEqual(got, want) {
		t.Errorf("test_station.Consumes = %v, want %v", got, want)
	}
	if got, want := res.Goods["test_good"].BasePrice, 42; got != want {
		t.Errorf("test_good.BasePrice = %d, want %d", got, want)
	}
}

// TestLoad_MalformedFilesDontCrash: a malformed JSON file in the
// content directory must be a warning, not a fatal error. This
// mirrors the engine-side loader's "lenient at runtime" contract.
func TestLoad_MalformedFilesDontCrash(t *testing.T) {
	tmp := t.TempDir()
	modDir := filepath.Join(tmp, "broken")
	if err := os.MkdirAll(filepath.Join(modDir, "factions"), 0o755); err != nil {
		t.Fatal(err)
	}
	manifest := `{"id": "broken", "name": "Broken", "version": "0.0.1", "provides": {}}`
	if err := os.WriteFile(filepath.Join(modDir, "manifest.json"), []byte(manifest), 0o644); err != nil {
		t.Fatal(err)
	}
	// Write a malformed faction file alongside a valid one.
	if err := os.WriteFile(filepath.Join(modDir, "factions", "bad.json"), []byte(`{not json`), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "factions", "good.json"), []byte(`{"id": "good", "name": "Good"}`), 0o644); err != nil {
		t.Fatal(err)
	}

	res, err := Load(tmp, nil)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if _, ok := res.Factions["good"]; !ok {
		t.Errorf("valid faction good was not loaded alongside the bad one")
	}
	if len(res.Warnings) == 0 {
		t.Errorf("malformed JSON should have produced a warning")
	}
}

// TestLoad_LiveContent is the integration test the brief asks for:
// load the actual godot/mods/reachlock content and assert the
// loader copes with whatever is there. The brief names Tib, Vex,
// Sorrow Station, Reaver Skiff, and the three goods as the live
// content set; the loader doesn't need to know about the
// characters, but the faction/location/good files that exist
// should be parsed without error.
//
// `go test` runs with the package directory as the working
// directory, so ".." doesn't reliably land at the repo root from
// a deeply-nested package. We walk up until we find the directory
// that contains both a server/ and a godot/ subdir, then use
// that as the mods root.
func TestLoad_LiveContent(t *testing.T) {
	res, err := Load(findRepoMods(t), nil)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	t.Logf("loaded %d mods, %d factions, %d locations, %d goods (warnings: %d)",
		len(res.Mods), len(res.Factions), len(res.Locations), len(res.Goods), len(res.Warnings))
	// The integration test asserts structural properties, not
	// specific ids — the engine must contain zero content ids
	// (ARCHITECTURE.md "Known boundary debt"), even in its tests.
	// Asserting "the loader found the right number of things" is
	// the engine-side check; asserting "Sorrow Station exists" is
	// a content-side check and belongs in the content repo.
	if len(res.Goods) == 0 {
		t.Errorf("no goods were loaded from the live content")
	}
	if len(res.Locations) == 0 {
		t.Errorf("no locations were loaded from the live content")
	}
	if len(res.Factions) == 0 {
		t.Errorf("no factions were loaded from the live content")
	}
	// Every good's base price must be a positive integer (the
	// schema requires >= 1) — this is the engine-side invariant
	// the loader promises to uphold.
	for id, g := range res.Goods {
		if g.BasePrice < 1 {
			t.Errorf("good %s has base_price %d, want >= 1", id, g.BasePrice)
		}
	}
}

func keysOf[V any](m map[string]V) []string {
	keys := make([]string, 0, len(m))
	for k := range m {
		keys = append(keys, k)
	}
	sort.Strings(keys)
	return keys
}

// findRepoMods walks up from this package's directory until it
// finds a godot/mods subdir, and returns its absolute path. Falls
// back to the loader's "no mods" path if the walk hits the
// filesystem root without finding one.
func findRepoMods(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for i := 0; i < 8; i++ {
		candidate := filepath.Join(dir, "godot", "mods")
		if info, err := os.Stat(candidate); err == nil && info.IsDir() {
			return candidate
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			break
		}
		dir = parent
	}
	t.Skip("could not locate godot/mods from the package directory — skipping live content test")
	return ""
}
