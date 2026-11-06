from pathlib import Path

from .files import read_json


def signed_in(data_dir: Path) -> bool:
    metadata = read_json(data_dir / "accounts.json")
    if isinstance(metadata, dict):
        accounts = metadata.get("accounts")
        return isinstance(accounts, list) and any(
            isinstance(account, dict) and not account.get("needsSignIn") for account in accounts
        )
    return (data_dir / "auth-state.json").is_file()
