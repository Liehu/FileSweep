package ai

import "context"

// FallbackEnricher tries primary first, falls back to secondary if confidence is low.
type FallbackEnricher struct {
	Primary   Enricher // e.g. offline
	Secondary Enricher // e.g. LLM
}

func NewFallbackEnricher(primary, secondary Enricher) *FallbackEnricher {
	return &FallbackEnricher{Primary: primary, Secondary: secondary}
}

func (f *FallbackEnricher) Name() string {
	return "fallback"
}

func (f *FallbackEnricher) Enrich(ctx context.Context, req EnrichRequest, categories []string) (EnrichResult, error) {
	// Try primary first
	if f.Primary != nil {
		result, err := f.Primary.Enrich(ctx, req, categories)
		if err == nil && result.Confidence >= 0.5 && result.Description != "" {
			return result, nil
		}
	}

	// Fallback to secondary
	if f.Secondary != nil {
		result, err := f.Secondary.Enrich(ctx, req, categories)
		if err == nil {
			return result, nil
		}
		return result, err
	}

	// Both unavailable
	return EnrichResult{Confidence: 0, NeedsReview: true, Provider: "fallback"}, nil
}
