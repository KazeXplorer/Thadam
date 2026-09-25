// Package logger provides a lightweight, human-readable slog.Handler for the
// chain service, modelled after the TMS engine logger.
//
// Output format:
//
//	2006/01/02 15:04:05 LEVEL [pre-attrs] MSG [call-attrs]
//
// Pre-attrs are fields attached via slog.With() (e.g. "network", "provider").
// Call-attrs are fields passed at the log call site.
package logger

import (
	"context"
	"fmt"
	"io"
	"log/slog"
	"os"
	"strconv"
	"sync"
	"time"
)

// Handler formats log lines as:
//
//	TIME LEVEL [pre-attrs] MSG [call-attrs]
type Handler struct {
	mu    sync.Mutex
	out   io.Writer
	level slog.Leveler
	attrs []slog.Attr
}

// bufPool reuses byte slices across log calls to avoid per-call allocations.
var bufPool = sync.Pool{
	New: func() any {
		b := make([]byte, 0, 256)
		return &b
	},
}

// New returns a Handler that writes to w at the given minimum level.
func New(w io.Writer, level slog.Leveler) *Handler {
	if w == nil {
		w = os.Stderr
	}
	if level == nil {
		level = slog.LevelInfo
	}
	return &Handler{out: w, level: level}
}

func (h *Handler) Enabled(_ context.Context, level slog.Level) bool {
	return level >= h.level.Level()
}

func (h *Handler) Handle(_ context.Context, r slog.Record) error {
	bp := bufPool.Get().(*[]byte)
	buf := (*bp)[:0]

	// TIME
	buf = r.Time.AppendFormat(buf, "2006/01/02 15:04:05")
	buf = append(buf, ' ')

	// LEVEL
	buf = appendLevel(buf, r.Level)

	// pre-attrs (from slog.With)
	for _, a := range h.attrs {
		buf = append(buf, ' ')
		buf = appendAttr(buf, a)
	}

	// message
	buf = append(buf, ' ')
	buf = append(buf, r.Message...)

	// call-site attrs
	r.Attrs(func(a slog.Attr) bool {
		buf = append(buf, ' ')
		buf = appendAttr(buf, a)
		return true
	})

	buf = append(buf, '\n')
	*bp = buf

	h.mu.Lock()
	_, err := h.out.Write(buf)
	h.mu.Unlock()

	bufPool.Put(bp)
	return err
}

func (h *Handler) WithAttrs(attrs []slog.Attr) slog.Handler {
	combined := make([]slog.Attr, len(h.attrs)+len(attrs))
	copy(combined, h.attrs)
	copy(combined[len(h.attrs):], attrs)
	return &Handler{out: h.out, level: h.level, attrs: combined}
}

func (h *Handler) WithGroup(_ string) slog.Handler { return h }

// ── helpers ───────────────────────────────────────────────────────────────────

func appendLevel(buf []byte, level slog.Level) []byte {
	switch level {
	case slog.LevelDebug:
		return append(buf, "DEBUG"...)
	case slog.LevelInfo:
		return append(buf, "INFO "...)
	case slog.LevelWarn:
		return append(buf, "WARN "...)
	case slog.LevelError:
		return append(buf, "ERROR"...)
	default:
		return append(buf, level.String()...)
	}
}

func appendAttr(buf []byte, a slog.Attr) []byte {
	buf = append(buf, a.Key...)
	buf = append(buf, '=')
	return appendValue(buf, a.Value.Resolve())
}

func appendValue(buf []byte, v slog.Value) []byte {
	switch v.Kind() {
	case slog.KindString:
		s := v.String()
		// quote strings that contain spaces so attrs stay parseable
		if containsSpace(s) {
			buf = append(buf, '"')
			buf = append(buf, s...)
			buf = append(buf, '"')
			return buf
		}
		return append(buf, s...)
	case slog.KindInt64:
		return strconv.AppendInt(buf, v.Int64(), 10)
	case slog.KindUint64:
		return strconv.AppendUint(buf, v.Uint64(), 10)
	case slog.KindFloat64:
		return strconv.AppendFloat(buf, v.Float64(), 'f', 3, 64)
	case slog.KindBool:
		return strconv.AppendBool(buf, v.Bool())
	case slog.KindDuration:
		return append(buf, v.Duration().String()...)
	case slog.KindTime:
		return v.Time().AppendFormat(buf, time.RFC3339)
	case slog.KindGroup:
		for i, ga := range v.Group() {
			if i > 0 {
				buf = append(buf, ' ')
			}
			buf = appendAttr(buf, ga)
		}
		return buf
	default:
		return fmt.Appendf(buf, "%v", v.Any())
	}
}

func containsSpace(s string) bool {
	for _, c := range s {
		if c == ' ' || c == '\t' {
			return true
		}
	}
	return false
}
