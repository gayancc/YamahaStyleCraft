"""One-shot helper: not part of product; transforms main.rs session access patterns."""
from pathlib import Path
import re

path = Path(__file__).resolve().parents[1] / "src-tauri" / "src" / "main.rs"
text = path.read_text(encoding="utf-8")

# Inject doc_label param into commands that lock state and touch docs.
# Safer approach: only rewrite field accesses to use a local `doc` variable
# and require each function to define `let label = resolve_doc_label(doc_label);`
# Manual cargo fix is more reliable for param injection.

# Replace common access patterns AFTER a line that sets label+doc.
# We'll instead add a compatibility shim on Studio for migration:

shim = '''
impl Studio {
    /// Temporary single-doc accessors — prefer docs HashMap + resolve_doc_label.
    fn legacy_main_mut(&mut self) -> Result<&mut DocSlot, String> {
        if !self.docs.contains_key("main") {
            return Err("Open a Style first".into());
        }
        Ok(self.docs.get_mut("main").unwrap())
    }
    fn legacy_main(&self) -> Result<&DocSlot, String> {
        self.docs.get("main").ok_or_else(|| "Open a Style first".into())
    }
}
'''

if "fn legacy_main_mut" not in text:
    # Insert after doc_slot_mut function
    needle = "fn doc_slot_mut<'a>(s: &'a mut Studio, label: &str) -> Result<&'a mut DocSlot, String> {\n    s.docs\n        .get_mut(label)\n        .ok_or_else(|| \"Open a Style first\".into())\n}"
    if needle not in text:
        raise SystemExit("needle not found")
    text = text.replace(needle, needle + "\n" + shim)

# Map old fields through legacy_main for first compile pass.
replacements = [
    ("s.pending_live = None;", "doc_slot_mut(&mut s, \"main\")?.pending_live = None;"),
]

# Broader: replace s.session / s.epoch etc with legacy accessors — too ambiguous.
# Instead rewrite to:
#   let doc = doc_slot_mut(&mut s, &label)?;
# and use doc.session

path.write_text(text, encoding="utf-8")
print("shim inserted", "legacy_main_mut" in text)
