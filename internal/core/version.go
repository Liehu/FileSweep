package core

import (
	"regexp"
	"strconv"

	"github.com/hashicorp/go-version"
)

var semverRe = regexp.MustCompile(`[-_\s]v?(\d+\.\d+\.\d+)`)
var simpleVerRe = regexp.MustCompile(`[-_\s]v?(\d+\.\d+)`)
var dateVerRe = regexp.MustCompile(`[-_\s](\d{8})`)
var buildNumRe = regexp.MustCompile(`[-_\s](\d+)$`)

func ExtractVersion(filename string) (string, bool) {
	name := stripExtension(filename)

	if m := semverRe.FindStringSubmatch(name); m != nil {
		return m[1], true
	}
	if m := simpleVerRe.FindStringSubmatch(name); m != nil {
		return m[1], true
	}
	if m := dateVerRe.FindStringSubmatch(name); m != nil {
		return m[1], true
	}
	if m := buildNumRe.FindStringSubmatch(name); m != nil {
		return m[1], true
	}
	return "", false
}

func stripExtension(name string) string {
	for _, ext := range []string{".tar.gz", ".tar.xz", ".tar.bz2"} {
		if len(name) > len(ext) && name[len(name)-len(ext):] == ext {
			return name[:len(name)-len(ext)]
		}
	}
	for i := len(name) - 1; i >= 0; i-- {
		if name[i] == '.' {
			return name[:i]
		}
	}
	return name
}

func CompareVersions(a, b string) int {
	if isDateVersion(a) && isDateVersion(b) {
		ai, _ := strconv.Atoi(a)
		bi, _ := strconv.Atoi(b)
		switch {
		case ai < bi:
			return -1
		case ai > bi:
			return 1
		default:
			return 0
		}
	}

	va, errA := version.NewVersion(a)
	vb, errB := version.NewVersion(b)
	if errA != nil || errB != nil {
		return 0
	}
	if va.LessThan(vb) {
		return -1
	}
	if va.GreaterThan(vb) {
		return 1
	}
	return 0
}

func isDateVersion(s string) bool {
	if len(s) != 8 {
		return false
	}
	for _, c := range s {
		if c < '0' || c > '9' {
			return false
		}
	}
	return true
}

func FindLatest(files []FileRecord) FileRecord {
	if len(files) == 0 {
		return FileRecord{}
	}

	type versioned struct {
		record  FileRecord
		version string
	}

	var versionedFiles []versioned
	var unversioned []FileRecord

	for _, f := range files {
		if v, ok := ExtractVersion(f.Name); ok {
			versionedFiles = append(versionedFiles, versioned{record: f, version: v})
		} else {
			unversioned = append(unversioned, f)
		}
	}

	if len(versionedFiles) > 0 {
		best := versionedFiles[0]
		for _, vf := range versionedFiles[1:] {
			if CompareVersions(vf.version, best.version) > 0 {
				best = vf
			} else if CompareVersions(vf.version, best.version) == 0 {
				if vf.record.ModTime.After(best.record.ModTime) {
					best = vf
				}
			}
		}
		return best.record
	}

	best := unversioned[0]
	for _, f := range unversioned[1:] {
		if f.ModTime.After(best.ModTime) {
			best = f
		}
	}
	return best
}
