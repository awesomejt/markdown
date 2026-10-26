// Disposable direct Go-library screen. Not a Swift/C ABI or production adapter.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"strings"
	"testing/fstest"
	"time"

	"github.com/d2lang/d2/d2graph"
	"github.com/d2lang/d2/d2layouts/d2dagrelayout"
	"github.com/d2lang/d2/d2lib"
	"github.com/d2lang/d2/d2renderers/d2svg"
	"github.com/d2lang/d2/lib/log"
	"github.com/d2lang/d2/lib/textmeasure"
)

func main() {
	results := []map[string]any{}
	cases := [][2]string{{"flowchart", "a: Start\nb: Done\na -> b"},
		{"sequence", "shape: sequence_diagram\nAlice -> Bob: Hello\nBob -> Alice: Reply"},
		{"invalid", "a -> {"}, {"import-denied", "...@go.mod"},
		{"remote-icon", "a: {icon: https://example.invalid/probe.png}"},
		{"unsafe-link", "a: {link: javascript:alert(1)}"}}
	for _, item := range cases {
		samples := []float64{}
		var svg []byte
		var failure string
		for i := 0; i < 6; i++ {
			start := time.Now()
			ctx, cancel := context.WithTimeout(log.WithDefault(context.Background()), 3*time.Second)
			ruler, err := textmeasure.NewRuler()
			if err != nil {
				panic(err)
			}
			opts := &d2lib.CompileOptions{Ruler: ruler, FS: fstest.MapFS{},
				LayoutResolver: func(string) (d2graph.LayoutGraph, error) { return d2dagrelayout.DefaultLayout, nil }}
			render := &d2svg.RenderOpts{}
			diagram, _, err := d2lib.Compile(ctx, item[1], opts, render)
			if err == nil {
				svg, err = d2svg.Render(diagram, render)
			}
			cancel()
			if err != nil {
				failure = err.Error()
			}
			if i > 0 {
				samples = append(samples, float64(time.Since(start).Nanoseconds())/1e6)
			}
		}
		results = append(results, map[string]any{"name": item[0], "samples_ms": samples,
			"error": failure, "output_bytes": len(svg), "svg_present": strings.Contains(string(svg), "<svg"),
			"has_remote_reference":     strings.Contains(string(svg), "https://example.invalid/"),
			"has_javascript_reference": strings.Contains(string(svg), "javascript:")})
	}
	if err := json.NewEncoder(os.Stdout).Encode(map[string]any{"engine": "d2-go-dagre", "results": results}); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
