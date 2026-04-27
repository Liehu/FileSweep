package core

import (
	"path/filepath"
	"strings"
)

type PrivacyChecker struct {
	Rules []string
}

func NewPrivacyChecker(rules []string) *PrivacyChecker {
	return &PrivacyChecker{Rules: rules}
}

func (p *PrivacyChecker) ShouldSkip(file FileRecord) bool {
	if file.AISkip {
		return true
	}
	// 匹配文件名或完整路径
	if p.Match(file.Name, file.LocalPath) {
		return true
	}
	// #18: 匹配父目录（用于排除整个目录如 confidential/）
	return p.Match("", filepath.Dir(file.LocalPath))
}

func (p *PrivacyChecker) Match(name string, path string) bool {
	for _, pattern := range p.Rules {
		if matchPattern(pattern, name) || matchPattern(pattern, path) {
			return true
		}
	}
	return false
}

func matchPattern(pattern, s string) bool {
	lower := strings.ToLower(s)
	p := strings.ToLower(pattern)

	if strings.Contains(p, "*") || strings.Contains(p, "?") {
		return globMatch(p, lower)
	}
	return strings.Contains(lower, p)
}

// globMatch 支持 * (单级) 和 ** (多级) 通配符。
func globMatch(pattern, s string) bool {
	// #17: 处理 ** 多级通配符
	if strings.Contains(pattern, "**") {
		return globMatchDoubleStar(pattern, s)
	}

	px, sx := 0, 0
	starIdx, matchIdx := -1, 0

	for sx < len(s) {
		if px < len(pattern) && (pattern[px] == s[sx] || pattern[px] == '?') {
			px++
			sx++
		} else if px < len(pattern) && pattern[px] == '*' {
			starIdx = px
			matchIdx = sx
			px++
		} else if starIdx != -1 {
			px = starIdx + 1
			matchIdx++
			sx = matchIdx
		} else {
			return false
		}
	}

	for px < len(pattern) && pattern[px] == '*' {
		px++
	}
	return px == len(pattern)
}

// globMatchDoubleStar handles ** patterns by splitting on ** and matching segments.
func globMatchDoubleStar(pattern, s string) bool {
	// Simple approach: replace ** with a special marker and use recursive matching
	segments := strings.Split(pattern, "**")
	if len(segments) == 1 {
		return globMatch(pattern, s)
	}

	// Each ** can match zero or more path segments
	return matchDoubleStarSegments(segments, s, 0, 0)
}

func matchDoubleStarSegments(segments []string, s string, segIdx, strIdx int) bool {
	if segIdx >= len(segments) {
		return strIdx >= len(s)
	}

	seg := segments[segIdx]
	isLast := segIdx == len(segments)-1

	// Try matching segment at every position from strIdx onwards
	for i := strIdx; i <= len(s); i++ {
		if globMatch(seg, s[strIdx:i]) {
			if isLast {
				// Last segment must match the remainder exactly
				if i == len(s) {
					return true
				}
			} else {
				if matchDoubleStarSegments(segments, s, segIdx+1, i) {
					return true
				}
			}
		}
	}
	return false
}
