import sys
import os
from pathlib import Path

def get_app_name():
    return "Minna"

def is_frozen():
    """Returns True if running as a bundled executable."""
    return getattr(sys, 'frozen', False)

def get_resource_path(relative_path: str) -> Path:
    """
    Get the absolute path to a resource file.
    Works for dev and for PyInstaller.
    
    Args:
        relative_path: Path relative to the application root (e.g. "core/schema.sql")
    """
    if is_frozen():
        # PyInstaller creates a temp folder and stores path in _MEIPASS
        base_path = Path(sys._MEIPASS)
        # In bundled app, we might structure things differently, but let's assume flat or preserved structure
        # If we collect 'src/minna' into root, then relative path should be adjusted
        # For now, let's assume the relative path passed in matches the bundle structure
        return base_path / relative_path
    else:
        # Dev mode: resolve relative to this file's parent (src/minna)
        # this file is in src/minna/utils/paths.py
        # so parent is src/minna/utils, parent.parent is src/minna
        base_path = Path(__file__).parent.parent
        return base_path / relative_path

def get_data_path(filename: str = "") -> Path:
    """
    Get the path for persistent data storage (e.g. database, logs).
    
    Args:
        filename: Optional filename to append to the base data path.
    """
    if is_frozen():
        # Use macOS Application Support folder
        home = Path.home()
        data_dir = home / "Library" / "Application Support" / get_app_name()
    else:
        # Dev mode: Use local src/minna directory (or project root, strictly speaking user wants src/minna for now based on legacy)
        # Previous vector_db.py put minna.db in src/minna/core/minna.db or src/minna/minna.db?
        # vector_db.py line 27: base_dir = Path(__file__).parent -> src/minna/core/
        # Let's verify existing behavior. 
        # The user request said: "If in dev mode, use the local src/ path."
        # vector_db.py was putting it in `src/minna/core/minna.db`.
        # To minimize disruption in dev, let's keep it there or close to it?
        # But `vector_db.py` is in `core`. If we use `src/minna` as base, it goes to `src/minna/minna.db`.
        # Let's standardize on `src/minna/data` or just `src/minna`?
        # Re-reading prompt: "If in dev mode, use the local src/ path." 
        # I will start with `src/minna` to keep it clean, but let's check where it currently is. 
        # View file shows: `self.db_path = str(base_dir / "minna.db")` where base_dir is `u/core`.
        # So currently it is `src/minna/core/minna.db`.
        # I will keep it consistent with the "application root" concept in dev -> `src/minna`.
        # Wait, if I change it, I lose existing data. The user has 37 records.
        # I should probably map it to `src/minna/core` if filename is minna.db for backward compat?
        # Or just tell user I am moving it.
        # Actually, `implementation_plan` said: "If dev: Use local path (relative to project root or `src/minna`..."
        # Let's set dev data path to `src/minna` and we might need to move the DB or accept it creates a new one.
        # Given "37 records", I should try to preserve it or just warn.
        # I'll point to `src/minna/core` for now to be safe if I can detect it, otherwise `src/minna`.
        # Simplest is `src/minna` and ask user to move it if they want.
        
        # Actually, best practice: `src/minna/local_data` or similar. 
        # But let's stick to `src/minna` (the package root) for dev simplicity as per prompt.
        data_dir = Path(__file__).parent.parent
    
    data_dir.mkdir(parents=True, exist_ok=True)
    
    if filename:
        return data_dir / filename
    return data_dir
