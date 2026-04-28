package core

import (
	"path/filepath"
	"regexp"
	"strings"
)

type DedupGroup struct {
	Representative FileRecord   `json:"representative"`
	Duplicates     []FileRecord `json:"duplicates"`
	Reason         string       `json:"reason"`
}

type DedupDetector struct {
	PreferUncompressed bool
	FuzzyThreshold     int
}

func NewDedupDetector(preferUncompressed bool, fuzzyThreshold int) *DedupDetector {
	if fuzzyThreshold <= 0 {
		fuzzyThreshold = 2
	}
	return &DedupDetector{
		PreferUncompressed: preferUncompressed,
		FuzzyThreshold:     fuzzyThreshold,
	}
}

func (d *DedupDetector) Detect(records []FileRecord) []DedupGroup {
	var groups []DedupGroup
	used := make(map[int]bool)

	groups = append(groups, d.exactHashMatch(records, used)...)
	groups = append(groups, d.versionGroupMatch(records, used)...)
	groups = append(groups, d.redundantArchiveMatch(records, used)...)
	groups = append(groups, d.sizeMatch(records, used)...)
	groups = append(groups, d.fuzzyNameMatch(records, used)...)

	return groups
}

func (d *DedupDetector) exactHashMatch(records []FileRecord, used map[int]bool) []DedupGroup {
	byHash := make(map[string][]int)
	for i, r := range records {
		if used[i] {
			continue
		}
		byHash[r.FileHash] = append(byHash[r.FileHash], i)
	}

	var groups []DedupGroup
	for _, indices := range byHash {
		if len(indices) < 2 {
			continue
		}

		repIdx := d.selectRepresentative(records, indices)
		var duplicates []FileRecord
		for _, idx := range indices {
			used[idx] = true
			if idx != repIdx {
				duplicates = append(duplicates, records[idx])
			}
		}

		if len(duplicates) > 0 {
			groups = append(groups, DedupGroup{
				Representative: records[repIdx],
				Duplicates:     duplicates,
				Reason:         "hash_match",
			})
		}
	}
	return groups
}

func (d *DedupDetector) sizeMatch(records []FileRecord, used map[int]bool) []DedupGroup {
	bySize := make(map[int64][]int)
	for i, r := range records {
		if used[i] {
			continue
		}
		bySize[r.FileSize] = append(bySize[r.FileSize], i)
	}

	var groups []DedupGroup
	for _, indices := range bySize {
		if len(indices) < 2 {
			continue
		}

		related := d.findFuzzyRelated(records, indices)
		for _, group := range related {
			if len(group) < 2 {
				continue
			}
			repIdx := d.selectRepresentative(records, group)
			var duplicates []FileRecord
			for _, idx := range group {
				used[idx] = true
				if idx != repIdx {
					duplicates = append(duplicates, records[idx])
				}
			}
			if len(duplicates) > 0 {
				groups = append(groups, DedupGroup{
					Representative: records[repIdx],
					Duplicates:     duplicates,
					Reason:         "size_only",
				})
			}
		}
	}
	return groups
}

func (d *DedupDetector) findFuzzyRelated(records []FileRecord, indices []int) [][]int {
	matched := make(map[int]bool)
	var groups [][]int

	for i, idxA := range indices {
		if matched[idxA] {
			continue
		}
		var group []int
		group = append(group, idxA)
		matched[idxA] = true

		for j := i + 1; j < len(indices); j++ {
			idxB := indices[j]
			if matched[idxB] {
				continue
			}
			nameA := normalizeName(records[idxA].Name)
			nameB := normalizeName(records[idxB].Name)
			if levenshtein(nameA, nameB) <= d.FuzzyThreshold {
				group = append(group, idxB)
				matched[idxB] = true
			}
		}

		groups = append(groups, group)
	}

	return groups
}

func (d *DedupDetector) fuzzyNameMatch(records []FileRecord, used map[int]bool) []DedupGroup {
	var unused []int
	for i := range records {
		if !used[i] {
			unused = append(unused, i)
		}
	}

	var groups []DedupGroup
	matched := make(map[int]bool)

	for i, idxA := range unused {
		if matched[idxA] {
			continue
		}
		var group []int
		group = append(group, idxA)

		for j := i + 1; j < len(unused); j++ {
			idxB := unused[j]
			if matched[idxB] {
				continue
			}
			nameA := normalizeName(records[idxA].Name)
			nameB := normalizeName(records[idxB].Name)
			if levenshtein(nameA, nameB) <= d.FuzzyThreshold {
				group = append(group, idxB)
			}
		}

		if len(group) >= 2 {
			repIdx := d.selectRepresentative(records, group)
			var duplicates []FileRecord
			for _, idx := range group {
				matched[idx] = true
				used[idx] = true
				if idx != repIdx {
					duplicates = append(duplicates, records[idx])
				}
			}
			if len(duplicates) > 0 {
				groups = append(groups, DedupGroup{
					Representative: records[repIdx],
					Duplicates:     duplicates,
					Reason:         "fuzzy_name",
				})
			}
		}
	}

	return groups
}

func (d *DedupDetector) selectRepresentative(records []FileRecord, indices []int) int {
	best := indices[0]

	for _, idx := range indices[1:] {
		if d.isBetterRepresentative(records[idx], records[best]) {
			best = idx
		}
	}

	return best
}

func (d *DedupDetector) isBetterRepresentative(a, b FileRecord) bool {
	if d.PreferUncompressed {
		aUncompressed := isUncompressed(a.Extension)
		bUncompressed := isUncompressed(b.Extension)
		if aUncompressed && !bUncompressed {
			return true
		}
		if !aUncompressed && bUncompressed {
			return false
		}
	}

	aVer, aOk := ExtractVersion(a.Name)
	bVer, bOk := ExtractVersion(b.Name)
	if aOk && bOk {
		cmp := CompareVersions(aVer, bVer)
		if cmp > 0 {
			return true
		}
		if cmp < 0 {
			return false
		}
	}
	if aOk && !bOk {
		return true
	}
	if !aOk && bOk {
		return false
	}

	return a.ModTime.After(b.ModTime)
}

func isUncompressed(ext string) bool {
	uncompressed := map[string]bool{
		".exe": true, ".msi": true, ".pkg": true,
		".dmg": true, ".deb": true, ".rpm": true,
		".jar": true, ".AppImage": true,
	}
	return uncompressed[strings.ToLower(ext)]
}

// versionGroupMatch groups files that share the same base name but differ in version.
// e.g. "Yakit-1.4.6-0417-windows-amd64.exe" and "Yakit-1.4.7-0424-windows-amd64.exe"
func (d *DedupDetector) versionGroupMatch(records []FileRecord, used map[int]bool) []DedupGroup {
	type keyed struct {
		idx  int
		base string
		ver  string
	}
	var versioned []keyed
	for i, r := range records {
		if used[i] {
			continue
		}
		ver, ok := ExtractVersion(r.Name)
		if !ok || ver == "" {
			continue
		}
		base := extractBaseName(r.Name)
		if base == "" {
			continue
		}
		versioned = append(versioned, keyed{idx: i, base: base, ver: ver})
	}

	byBase := make(map[string][]keyed)
	for _, v := range versioned {
		key := v.base + filepath.Ext(records[v.idx].Name)
		byBase[key] = append(byBase[key], v)
	}

	var groups []DedupGroup
	for _, items := range byBase {
		if len(items) < 2 {
			continue
		}
		indices := make([]int, len(items))
		for i, v := range items {
			indices[i] = v.idx
		}
		repIdx := d.selectRepresentative(records, indices)
		var duplicates []FileRecord
		for _, idx := range indices {
			used[idx] = true
			if idx != repIdx {
				duplicates = append(duplicates, records[idx])
			}
		}
		if len(duplicates) > 0 {
			groups = append(groups, DedupGroup{
				Representative: records[repIdx],
				Duplicates:     duplicates,
				Reason:         "multi_version",
			})
		}
	}
	return groups
}

// extractBaseName strips version, platform, arch, and date suffixes from a filename
// to produce a canonical software base name.
// "Yakit-1.4.6-0417-windows-amd64.exe" -> "yakit"
func extractBaseName(filename string) string {
	name := stripExtension(filename)
	// Remove everything from the first version-like segment onward
	re := regexp.MustCompile(`[-_\s]v?\d[\d.]*`)
	loc := re.FindStringIndex(name)
	if loc != nil {
		name = name[:loc[0]]
	}
	name = strings.ToLower(strings.TrimSpace(name))
	return name
}

func normalizeName(name string) string {
	base := name
	ext := filepath.Ext(name)
	if ext != "" {
		base = name[:len(name)-len(ext)]
	}
	base = strings.ToLower(base)
	for _, sep := range []string{"-", "_", ".", " "} {
		base = strings.ReplaceAll(base, sep, "")
	}
	for _, suffix := range []string{"setup", "install", "installer", "win64", "win32", "amd64", "x64", "x86", "64bit", "32bit"} {
		base = strings.TrimSuffix(base, suffix)
	}
	return base
}

func levenshtein(a, b string) int {
	la, lb := len(a), len(b)
	if la == 0 {
		return lb
	}
	if lb == 0 {
		return la
	}

	prev := make([]int, lb+1)
	curr := make([]int, lb+1)

	for j := 0; j <= lb; j++ {
		prev[j] = j
	}

	for i := 1; i <= la; i++ {
		curr[0] = i
		for j := 1; j <= lb; j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			curr[j] = min3(
				prev[j]+1,
				curr[j-1]+1,
				prev[j-1]+cost,
			)
		}
		prev, curr = curr, prev
	}

	return prev[lb]
}

func min3(a, b, c int) int {
	if a < b {
		if a < c {
			return a
		}
		return c
	}
	if b < c {
		return b
	}
	return c
}

// redundantArchiveMatch detects archive files (.zip, .7z, .rar, etc.) that are
// redundant because an unpackaged version (.exe, .msi, etc.) of the same software exists.
// e.g. "fabric-windows-amd64.exe" vs "fabric.7z"
func (d *DedupDetector) redundantArchiveMatch(records []FileRecord, used map[int]bool) []DedupGroup {
	archiveExts := map[string]bool{
		".zip": true, ".7z": true, ".rar": true, ".gz": true,
		".tar": true, ".xz": true, ".bz2": true,
	}

	type entry struct {
		idx       int
		normalized string
	}

	var archives []entry
	var nonArchives []entry

	for i, r := range records {
		if used[i] {
			continue
		}
		ext := strings.ToLower(r.Extension)
		nameLower := strings.ToLower(r.Name)
		isArchive := archiveExts[ext] ||
			strings.HasSuffix(nameLower, ".tar.gz") ||
			strings.HasSuffix(nameLower, ".tar.xz") ||
			strings.HasSuffix(nameLower, ".tar.bz2")

		norm := normalizeForArchiveMatch(r.Name)
		if norm == "" {
			continue
		}

		if isArchive {
			archives = append(archives, entry{idx: i, normalized: norm})
		} else {
			nonArchives = append(nonArchives, entry{idx: i, normalized: norm})
		}
	}

	var groups []DedupGroup
	usedArchive := make(map[int]bool)

	for _, arch := range archives {
		if usedArchive[arch.idx] {
			continue
		}
		for _, nonArch := range nonArchives {
			if arch.normalized == nonArch.normalized {
				repIdx := nonArch.idx // prefer the non-archive as representative
				dupIdx := arch.idx
				if d.isBetterRepresentative(records[repIdx], records[dupIdx]) {
					// keep non-archive as rep
				} else {
					repIdx, dupIdx = dupIdx, repIdx
				}
				used[repIdx] = true
				used[dupIdx] = true
				usedArchive[arch.idx] = true
				groups = append(groups, DedupGroup{
					Representative: records[repIdx],
					Duplicates:     []FileRecord{records[dupIdx]},
					Reason:         "redundant_archive",
				})
				break
			}
		}
	}
	return groups
}

// normalizeForArchiveMatch produces a canonical name for comparing
// archive vs non-archive versions of the same software.
func normalizeForArchiveMatch(name string) string {
	base := name
	ext := filepath.Ext(name)
	if ext != "" {
		base = name[:len(name)-len(ext)]
	}
	// Strip .tar from double extensions like .tar.gz
	if strings.HasSuffix(strings.ToLower(base), ".tar") {
		base = base[:len(base)-4]
	}
	base = strings.ToLower(base)
	for _, sep := range []string{"-", "_", ".", " "} {
		base = strings.ReplaceAll(base, sep, "")
	}
	for _, suffix := range []string{
		"setup", "install", "installer", "update",
		"win64", "win32", "windowsamd64", "windowsx64", "windowsx86",
		"amd64", "x64", "x86", "64bit", "32bit",
		"linuxamd64", "linuxx64", "darwinamd64", "darwinx64",
	} {
		base = strings.TrimSuffix(base, suffix)
	}
	return base
}
