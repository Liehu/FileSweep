package core

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"time"
)

type FileRecord struct {
	ID                 string    `json:"id"`
	Name               string    `json:"name"`
	Version            string    `json:"version"`
	Category           string    `json:"category"`
	LocalPath          string    `json:"localPath"`
	FileSize           int64     `json:"fileSize"`
	FileHash           string    `json:"fileHash"`
	Extension          string    `json:"extension"`
	FunctionalCategory string    `json:"functionalCategory"`
	Status             string    `json:"status"`
	AISkip             bool      `json:"aiSkip"`
	ScannedAt          time.Time `json:"scannedAt"`
	ModTime            time.Time `json:"modTime"`
	CatalogID          string    `json:"catalogId"`
	IsAppDir           bool      `json:"isAppDir"`
	AppDirPath         string    `json:"appDirPath"`
	AppDirReason       string    `json:"appDirReason"`
}

func NewFileRecordID(hash string, _ int64) string {
	return "rec_" + hash[:16]
}

func NewFileRecordIDWithPath(hash string, path string) string {
	h := sha256.Sum256([]byte(hash + path))
	return "rec_" + fmt.Sprintf("%x", h[:8])
}

func FileHashFromBytes(data []byte) string {
	h := sha256.Sum256(data)
	return hex.EncodeToString(h[:])
}

type CatalogEntry struct {
	ID                 string    `json:"id"`
	Name               string    `json:"name"`
	Description        string    `json:"description"`
	HomepageURL        string    `json:"homepageUrl"`
	DownloadURL        string    `json:"downloadUrl"`
	LatestVersion      string    `json:"latestVersion"`
	License            string    `json:"license"`
	FunctionalCategory string    `json:"functionalCategory"`
	Tags               []string  `json:"tags"`
	AIConfidence       float64   `json:"aiConfidence"`
	AIProvider         string    `json:"aiProvider"`
	MetaUpdatedAt      time.Time `json:"metaUpdatedAt"`
	Notes              string    `json:"notes"`
	NeedsReview        bool      `json:"needsReview"`
	AISkip             bool      `json:"aiSkip"`
}

type OperationLog struct {
	ID         int64     `json:"id"`
	Timestamp  time.Time `json:"timestamp"`
	Operation  Operation `json:"operation"`
	SourcePath string    `json:"sourcePath"`
	DestPath   string    `json:"destPath"`
	Reason     string    `json:"reason"`
	FileHash   string    `json:"fileHash"`
	FileSize   int64     `json:"fileSize"`
	Status     string    `json:"status"`
	SessionID  string    `json:"sessionId"`
	CanRevert  bool      `json:"canRevert"`
	RecordID   string    `json:"recordId"`
}

type ScanProgress struct {
	Total       int    `json:"total"`
	Done        int    `json:"done"`
	CurrentFile string `json:"currentFile"`
	Stage       string `json:"stage"`
}

type EnrichProgress struct {
	Total       int     `json:"total"`
	Done        int     `json:"done"`
	NeedsReview int     `json:"needsReview"`
	CurrentName string  `json:"currentName"`
}

type ClassifyResult struct {
	Category  string `json:"category"`
	TargetDir string `json:"target_dir"`
}
