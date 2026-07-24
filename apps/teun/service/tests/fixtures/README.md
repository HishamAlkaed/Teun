# Test fixtures

## `two_page.pdf`

Minimal 2-page text-only PDF used by the native `#[ignore]`d tests in
`src/rag/extract.rs` (requires `libpdfium.so`). Page 1 contains two text lines
("Pagina een regel een", "Pagina een regel twee"); page 2 contains one line
("Pagina twee regel een").

Generated deterministically (905 bytes, no compression, hand-built xref) with
the following Python script:

```python
#!/usr/bin/env python3
import sys

content1 = (
    b"BT /F1 12 Tf 72 720 Td (Pagina een regel een) Tj "
    b"0 -20 Td (Pagina een regel twee) Tj ET"
)
content2 = b"BT /F1 12 Tf 72 720 Td (Pagina twee regel een) Tj ET"

objs = {
    1: b"<< /Type /Catalog /Pages 2 0 R >>",
    2: b"<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>",
    3: (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 7 0 R >> >> /Contents 4 0 R >>"),
    4: b"<< /Length %d >>\nstream\n" % len(content1) + content1 + b"\nendstream",
    5: (b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 7 0 R >> >> /Contents 6 0 R >>"),
    6: b"<< /Length %d >>\nstream\n" % len(content2) + content2 + b"\nendstream",
    7: b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
}

out = b"%PDF-1.4\n"
offsets = {}
for n in range(1, 8):
    offsets[n] = len(out)
    out += b"%d 0 obj\n" % n + objs[n] + b"\nendobj\n"
xref_off = len(out)
out += b"xref\n0 8\n0000000000 65535 f \n"
for n in range(1, 8):
    out += b"%010d 00000 n \n" % offsets[n]
out += b"trailer\n<< /Size 8 /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n" % xref_off

with open(sys.argv[1], "wb") as f:
    f.write(out)
```

Run as `python gen_fixture.py tests/fixtures/two_page.pdf`.
