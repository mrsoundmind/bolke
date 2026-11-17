# /// script
# requires-python = ">=3.10"
# dependencies = ["huggingface_hub[hf_xet]>=0.34"]
# ///
"""
Copy Bolke's speech models from their original Hugging Face repos into
Bolke's own account, one model at a time.

For each model it copies the one quant the app downloads (checked against the
catalog's sha256) plus the repo's small text files (model card, licence), then
writes `copied-models.json`: catalog id -> new repo and revision. Feed that to
`bun scripts/use-copied-models.ts` to point the catalog at the copies.

Safe to rerun: a model whose file is already in the target repo with the same
sha256 is not uploaded again.

Env: HF_TOKEN (write access to TARGET_OWNER), TARGET_OWNER (e.g. mrsoundmind)
"""

import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
from pathlib import Path

from huggingface_hub import HfApi, hf_hub_download

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "src-tauri/src/catalog/catalog.json"
MODELS = json.loads((ROOT / "scripts/bolke-models.json").read_text())["models"]
OWNER = os.environ["TARGET_OWNER"]
TOKEN = os.environ["HF_TOKEN"]
SMALL_FILE_LIMIT = 1_000_000  # model cards, licences, configs

api = HfApi(token=TOKEN)


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 24), b""):
            h.update(chunk)
    return h.hexdigest()


def rebrand_card(text: str, source: str, target: str) -> str:
    text = text.replace(source, target)
    # Keep the words of links into Handy's sites, drop the links themselves.
    handy_url = r"https?://(?:[\w.-]*\.)?(?:handy\.computer|github\.com/handy-computer|github\.com/cjpais)[^\s)\]>]*"
    text = re.sub(r"\[([^\]]*)\]\(" + handy_url + r"\)", r"\1", text)
    text = re.sub(handy_url, "", text)
    return text.replace("handy-computer", OWNER).replace("Handy", "Bolke")


def target_has(repo: str, filename: str, digest: str) -> bool:
    try:
        infos = api.get_paths_info(repo, [filename], repo_type="model")
    except Exception:
        return False
    return any(
        getattr(i, "lfs", None) is not None and i.lfs.sha256 == digest for i in infos
    )


def main() -> int:
    catalog = {m["id"]: m for m in json.loads(CATALOG.read_text())["models"]}
    results = {}
    failures = []
    for model_id in MODELS:
        m = catalog[model_id]
        source, revision = m["repository"], m["revision"]
        name = source.split("/", 1)[1]
        target = f"{OWNER}/{name}"
        want = m["files"][m["quant"]]
        print(f"== {model_id}: {source}@{revision[:8]} -> {target}", flush=True)
        try:
            api.create_repo(target, repo_type="model", exist_ok=True, private=False)
            if target_has(target, want["filename"], want["sha256"]):
                print("   already copied", flush=True)
            else:
                work = Path(tempfile.mkdtemp(prefix="bolke-copy-"))
                try:
                    infos = api.list_repo_tree(
                        source, revision=revision, recursive=True, repo_type="model"
                    )
                    small = [
                        i.path
                        for i in infos
                        if getattr(i, "size", None) is not None
                        and i.size <= SMALL_FILE_LIMIT
                        and not i.path.endswith(".gguf")
                        and not i.path.startswith(".")
                    ]
                    for path in small + [want["filename"]]:
                        hf_hub_download(
                            source,
                            path,
                            revision=revision,
                            local_dir=work,
                            token=TOKEN,
                        )
                    got = sha256(work / want["filename"])
                    if got != want["sha256"]:
                        raise RuntimeError(f"sha256 mismatch: {got}")
                    for path in small:
                        p = work / path
                        if p.suffix.lower() in (".md", ".txt", ""):
                            try:
                                p.write_text(rebrand_card(p.read_text(), source, target))
                            except UnicodeDecodeError:
                                pass
                    cache = work / ".cache"
                    if cache.exists():
                        shutil.rmtree(cache)
                    api.upload_folder(
                        folder_path=str(work),
                        repo_id=target,
                        repo_type="model",
                        commit_message=f"Copy {name} for Bolke",
                    )
                    print(f"   uploaded {want['filename']}", flush=True)
                finally:
                    shutil.rmtree(work, ignore_errors=True)
            new_revision = api.model_info(target).sha
            results[model_id] = {"repository": target, "revision": new_revision}
            print(f"   revision {new_revision}", flush=True)
        except Exception as e:  # keep going; report at the end
            print(f"   FAILED: {e}", flush=True)
            failures.append(model_id)
    out = ROOT / "copied-models.json"
    out.write_text(json.dumps(results, indent=2) + "\n")
    print(f"\nCopied {len(results)} of {len(MODELS)} models -> {out}")
    if failures:
        print("Failed: " + ", ".join(failures))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
