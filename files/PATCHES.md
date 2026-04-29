# FileSweep Code Review & Feature Patches

## Bugs Found During Review

### Bug 1: Double empty state in LogsView.vue
`LogsView.vue` renders both a `<tr>` with "暂无日志记录" AND an `<n-empty>` component when logs are empty.
Fix: Remove the redundant `<n-empty>` (the table row is sufficient and styled correctly).

### Bug 2: AI category/tag enforcement missing in StartEnrich
`handlers.go` `StartEnrich` passes category names to AI as hints, but never validates the AI response
against the allowed set. AI can return arbitrary categories/tags, causing catalog sprawl.
Fix: After `BatchEnrich`, normalize each result's `FunctionalCategory` and `Tags` against the
allowed sets from the DB (tags table) and categories.yaml.

### Bug 3: `offline.go` over-aggressive name truncation
`normalizeForMatch` truncates to 8 chars: `if len(base) > 8 { base = base[:8] }`.
This causes "chromium" and "chrome" to both map to "chromium", creating false positives.
Fix: Increase to 12 characters, or remove the truncation entirely (rely on the separator stripping).

### Bug 4: `filterMultiVersion` double O(n²) scan in FileListView.vue
The computed `filteredFiles` re-scans all files with nested loops every render.
Fix: Memoize the base→count map outside the filter, computing it once per `store.files` change.

### Bug 5: LogsView pagination missing
`GetLogs` supports pagination but `LogsView.vue` hardcodes no page controls.
The `page_size` defaults to 50 in the API, so logs beyond 50 are invisible.
Fix: Add basic pagination or increase default page_size to 500 for log view.

### Bug 6: `catalog_entries` missing `tags` field in UpdateCatalog JSON binding  
`UpdateCatalog` uses `c.ShouldBindJSON(&entry)` where `core.CatalogEntry` has `Tags []string`
with `json:"tags"` — this works, but the frontend `EnrichView.vue` `saveEdit()` sends
`tags: form.tags` which is correct. However CatalogView doesn't expose tag editing.
(Minor — tracked as enhancement below.)

### Bug 7: WebSocket reconnect in EnrichView loses progress state
When WS reconnects (from `ws.onclose` setTimeout), `progress.percent` doesn't reset,
leaving stale "100%" displayed if a previous run completed.
Fix: Reset `progress` on new `startEnrich()` call (already done) — but also guard 
the reconnect handler to not overwrite running state.

---

## New Features Implementation

### Feature 1: Catalog Export (CSV + Obsidian MD)
**Files changed:**
- `frontend/src/views/CatalogView.vue` — replaced entirely (see CatalogView.vue artifact)

**What's new:**
- Export dropdown button with two options
- **CSV export**: All fields including tags (semicolon-separated), confidence %, dates
- **Obsidian MD export**: YAML frontmatter + grouped by functionalCategory + wiki-style tags
  (`#tag_name` format), table per entry with version/license/URLs

### Feature 2: AI Tag/Category Constraint
**Files changed:**
- `internal/db/tags.go` — NEW file (see tags.go artifact)
- `internal/server/handlers.go` — `StartEnrich` modified (see patch below)
- `internal/server/server.go` — routes added (see patch below)

**What's new:**
- Tags are stored in a new `tags` table in catalog.db
- Default tags are seeded on first access (`SeedDefaultTags`)
- In `StartEnrich`, after enrichment, `NormalizeTags` filters AI tags to only allowed ones
- If no AI tag matches allowed set → `["others"]`
- `NormalizeFunctionalCategory` maps AI category to allowed list or "others"
- Allowed categories loaded from `categories.yaml` (name field only, stripped of keywords)

### Feature 3: Tag Management UI
**Files changed:**
- `frontend/src/views/TagsView.vue` — NEW file (see TagsView.vue artifact)
- `frontend/src/router/index.ts` — added `/tags` route

**What's new:**
- Full CRUD for tags with color picker + 12 preset colors
- Inline editing in table
- Usage count (how many catalog entries reference each tag)
- Info box explaining the constraint behavior

---

## Exact Code Patches

### handlers.go — StartEnrich: add constraint normalization

After the line:
```go
results, batchErr := ai.BatchEnrich(...)
```

Replace the result processing loop with:

```go
// Load allowed tags and categories for constraint enforcement
allowedTags, _ := h.DB.GetTagNames()
// Seed defaults if empty
if len(allowedTags) == 0 {
    h.DB.SeedDefaultTags()
    allowedTags, _ = h.DB.GetTagNames()
}

// Load allowed functional categories from categories.yaml
var allowedCategories []string
categoriesPath = filepath.Join(filepath.Dir(h.Cfg.DBPath), "categories.yaml") // already computed above
// (catNames was already loaded — extract just the name part before " (")
for _, cn := range catNames {
    if idx := strings.Index(cn, " ("); idx > 0 {
        allowedCategories = append(allowedCategories, cn[:idx])
    } else {
        allowedCategories = append(allowedCategories, cn)
    }
}

var savedCount, skippedCount int
for i, result := range results {
    if result.Description == "" || result.Confidence == 0 {
        skippedCount++
        continue
    }

    // ── CONSTRAINT ENFORCEMENT ──
    result.Tags = db.NormalizeTags(result.Tags, allowedTags)
    result.FunctionalCategory = db.NormalizeFunctionalCategory(result.FunctionalCategory, allowedCategories)

    entry := core.CatalogEntry{
        ID:                 fmt.Sprintf("cat_%s", enrichable[i].ID),
        Name:               enrichable[i].Name,
        Description:        result.Description,
        HomepageURL:        result.HomepageURL,
        DownloadURL:        result.DownloadURL,
        LatestVersion:      result.LatestVersion,
        License:            result.License,
        FunctionalCategory: result.FunctionalCategory,
        Tags:               result.Tags,
        AIConfidence:       result.Confidence,
        AIProvider:         result.Provider,
        NeedsReview:        result.NeedsReview,
        MetaUpdatedAt:      time.Now().UTC(),
    }
    h.DB.InsertCatalogEntry(entry)

    if result.FunctionalCategory != "" {
        h.DB.UpdateFileFunctionalCategory(enrichable[i].ID, result.FunctionalCategory)
    }
    savedCount++
}
```

### server.go — Add tag routes

In the `api` group, add:
```go
// Tags
api.GET("/tags", handlers.GetTags)
api.POST("/tags", handlers.CreateTag)
api.PUT("/tags/:id", handlers.UpdateTag)
api.DELETE("/tags/:id", handlers.DeleteTag)
```

### LogsView.vue — Remove duplicate empty state

Remove these lines (the n-empty is redundant with the table's empty row):
```html
<n-empty v-if="!loading && logs.length === 0" description="暂无日志记录" style="padding: 40px 0" />
```

### offline.go — Fix over-aggressive truncation

Change:
```go
if len(base) > 8 {
    base = base[:8]
}
```
To:
```go
if len(base) > 12 {
    base = base[:12]
}
```

### App.vue — Add Tags nav item

In `bottomNavItems`:
```js
const bottomNavItems: NavItem[] = [
  { label: '扫描', icon: 'search', route: '/scan' },
  { label: '软件目录', icon: 'book-open', route: '/catalog' },
  { label: 'AI 丰富', icon: 'sparkles', route: '/enrich' },
  { label: '标签管理', icon: 'tag', route: '/tags' },   // ← NEW
  { label: '操作日志', icon: 'list', route: '/logs' },
  { label: '设置', icon: 'settings', route: '/settings' },
]
```
