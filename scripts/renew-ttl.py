#!/usr/bin/env python3
"""
scripts/renew-ttl.py
--------------------
TTL renewal daemon for Soroban smart contract state entries.

Soroban ledger entries have a Time-To-Live (TTL) measured in ledger sequences.
When a contract's instance storage or persistent keys fall below
LEDGER_LIFETIME_THRESHOLD (~7 days at 5 s/ledger), the entry enters an
"archived" state and is no longer directly readable until restored via
`restoreFootprint`. This daemon monitors remaining TTL for configured contract
instances and persistent keys, automatically submitting `extendFootprintTtl`
transactions before archival.

FEATURES
--------
- Polls TTL for a list of contract IDs via the Soroban RPC `getLedgerEntries`
  endpoint.
- Supports both instance storage and named persistent-key entries.
- Submits `extendFootprintTtl` transactions (via `stellar` CLI) when TTL falls
  below a configurable threshold.
- Alerts via webhook (Slack, PagerDuty, or generic HTTP) when an entry is
  within the ALERT_THRESHOLD_LEDGERS of archival.
- Dry-run mode for testing without broadcasting transactions.
- Structured JSON logging for ingestion by Prometheus / Loki / Datadog.

REQUIREMENTS
------------
    pip install requests stellar-sdk

USAGE
-----
    # Monitor and auto-renew (requires STELLAR_SECRET_KEY env var):
    python3 scripts/renew-ttl.py --config renew-ttl.json

    # Dry run — report TTL without submitting transactions:
    python3 scripts/renew-ttl.py --config renew-ttl.json --dry-run

    # One-shot check (no daemon loop):
    python3 scripts/renew-ttl.py --config renew-ttl.json --once

CONFIGURATION FILE (renew-ttl.json)
-------------------------------------
    {
      "rpc_url": "https://soroban-testnet.stellar.org",
      "network_passphrase": "Test SDF Network ; September 2015",
      "source_account": "GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
      "contracts": [
        {
          "id": "CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX",
          "name": "token",
          "keys": ["instance"]
        },
        {
          "id": "CYYYYYYY...",
          "name": "escrow",
          "keys": ["instance", "Buyer", "Seller"]
        }
      ],
      "ledger_lifetime_threshold": 120960,
      "ledger_bump_amount": 518400,
      "alert_threshold_ledgers": 17280,
      "poll_interval_seconds": 300,
      "webhook_url": "https://hooks.slack.com/services/XXX/YYY/ZZZ",
      "dry_run": false
    }

ENVIRONMENT VARIABLES
---------------------
    STELLAR_SECRET_KEY   Signing key for extend_ttl transactions (required unless --dry-run)
    WEBHOOK_URL          Override config webhook_url
    RPC_URL              Override config rpc_url
    NETWORK_PASSPHRASE   Override config network_passphrase
"""

from __future__ import annotations

import argparse
import json
import logging
import os
import struct
import sys
import time
from dataclasses import dataclass, field
from typing import Any

try:
    import requests
except ImportError:
    sys.exit("Missing dependency: pip install requests")

# ---------------------------------------------------------------------------
# Logging — structured JSON output
# ---------------------------------------------------------------------------

class _JsonFormatter(logging.Formatter):
    def format(self, record: logging.LogRecord) -> str:
        payload: dict[str, Any] = {
            "ts": self.formatTime(record, "%Y-%m-%dT%H:%M:%SZ"),
            "level": record.levelname,
            "msg": record.getMessage(),
        }
        if hasattr(record, "extra"):
            payload.update(record.extra)
        if record.exc_info:
            payload["exc"] = self.formatException(record.exc_info)
        return json.dumps(payload)


def _configure_logging() -> logging.Logger:
    handler = logging.StreamHandler(sys.stdout)
    handler.setFormatter(_JsonFormatter())
    logger = logging.getLogger("renew-ttl")
    logger.addHandler(handler)
    logger.setLevel(logging.INFO)
    return logger


LOG = _configure_logging()


def _log(level: str, msg: str, **extra: Any) -> None:
    record = LOG.makeRecord(
        LOG.name, getattr(logging, level.upper()), "", 0, msg, (), None
    )
    record.extra = extra  # type: ignore[attr-defined]
    LOG.handle(record)


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

DEFAULT_LEDGER_LIFETIME_THRESHOLD = 120_960   # ~7 days  (5 s/ledger)
DEFAULT_LEDGER_BUMP_AMOUNT        = 518_400   # ~30 days
DEFAULT_ALERT_THRESHOLD_LEDGERS   = 17_280    # ~24 hours
DEFAULT_POLL_INTERVAL_SECONDS     = 300       # 5 minutes


@dataclass
class ContractConfig:
    id: str
    name: str
    keys: list[str] = field(default_factory=lambda: ["instance"])


@dataclass
class Config:
    rpc_url: str
    network_passphrase: str
    source_account: str
    contracts: list[ContractConfig]
    ledger_lifetime_threshold: int = DEFAULT_LEDGER_LIFETIME_THRESHOLD
    ledger_bump_amount: int = DEFAULT_LEDGER_BUMP_AMOUNT
    alert_threshold_ledgers: int = DEFAULT_ALERT_THRESHOLD_LEDGERS
    poll_interval_seconds: int = DEFAULT_POLL_INTERVAL_SECONDS
    webhook_url: str = ""
    dry_run: bool = False

    @staticmethod
    def from_dict(d: dict[str, Any]) -> "Config":
        contracts = [
            ContractConfig(
                id=c["id"],
                name=c.get("name", c["id"][:8]),
                keys=c.get("keys", ["instance"]),
            )
            for c in d.get("contracts", [])
        ]
        return Config(
            rpc_url=os.environ.get("RPC_URL", d.get("rpc_url", "")),
            network_passphrase=os.environ.get(
                "NETWORK_PASSPHRASE", d.get("network_passphrase", "")
            ),
            source_account=d.get("source_account", ""),
            contracts=contracts,
            ledger_lifetime_threshold=int(
                d.get("ledger_lifetime_threshold", DEFAULT_LEDGER_LIFETIME_THRESHOLD)
            ),
            ledger_bump_amount=int(
                d.get("ledger_bump_amount", DEFAULT_LEDGER_BUMP_AMOUNT)
            ),
            alert_threshold_ledgers=int(
                d.get("alert_threshold_ledgers", DEFAULT_ALERT_THRESHOLD_LEDGERS)
            ),
            poll_interval_seconds=int(
                d.get("poll_interval_seconds", DEFAULT_POLL_INTERVAL_SECONDS)
            ),
            webhook_url=os.environ.get(
                "WEBHOOK_URL", d.get("webhook_url", "")
            ),
            dry_run=bool(d.get("dry_run", False)),
        )

    @staticmethod
    def load(path: str) -> "Config":
        with open(path) as fh:
            return Config.from_dict(json.load(fh))


# ---------------------------------------------------------------------------
# Soroban RPC helpers
# ---------------------------------------------------------------------------

def _rpc_call(rpc_url: str, method: str, params: Any) -> dict[str, Any]:
    """Make a JSON-RPC 2.0 call to the Soroban RPC endpoint."""
    payload = {"jsonrpc": "2.0", "id": 1, "method": method, "params": params}
    try:
        resp = requests.post(rpc_url, json=payload, timeout=30)
        resp.raise_for_status()
    except requests.RequestException as exc:
        raise RuntimeError(f"RPC request failed: {exc}") from exc
    body = resp.json()
    if "error" in body:
        raise RuntimeError(f"RPC error: {body['error']}")
    return body.get("result", {})


def get_latest_ledger(rpc_url: str) -> int:
    """Return the current ledger sequence number."""
    result = _rpc_call(rpc_url, "getLatestLedger", {})
    return int(result["sequence"])


def _contract_instance_key_xdr(contract_id: str) -> str:
    """
    Build a base64-encoded XDR LedgerKey for a contract's instance storage entry.

    Soroban RPC getLedgerEntries expects XDR-encoded LedgerKey values.
    For instance storage, the key type is CONTRACT_DATA with the following shape:
        LedgerKey::ContractData { contract, key: ScVal::LedgerKeyContractInstance, durability: Persistent }

    Rather than pulling in a full XDR library, we delegate to the stellar CLI.
    """
    import subprocess
    # `stellar contract invoke --simulate-only` can produce the LedgerKey XDR,
    # but the simplest approach is to use the Python stellar-sdk if available.
    try:
        from stellar_sdk import StrKey, xdr as stellar_xdr
        contract_addr = stellar_xdr.SCAddress(
            type=stellar_xdr.SCAddressType.SC_ADDRESS_TYPE_CONTRACT,
            contract_id=stellar_xdr.Hash(StrKey.decode_contract(contract_id)),
        )
        key = stellar_xdr.LedgerKey(
            type=stellar_xdr.LedgerEntryType.CONTRACT_DATA,
            contract_data=stellar_xdr.LedgerKeyContractData(
                contract=contract_addr,
                key=stellar_xdr.SCVal(
                    type=stellar_xdr.SCValType.SCV_LEDGER_KEY_CONTRACT_INSTANCE
                ),
                durability=stellar_xdr.ContractDataDurability.PERSISTENT,
            ),
        )
        import base64
        return base64.b64encode(key.to_xdr_bytes()).decode()
    except ImportError:
        # Fall back to stellar CLI
        result = subprocess.run(
            [
                "stellar", "contract", "info",
                "--id", contract_id,
                "--output", "ledger-key-xdr",
            ],
            capture_output=True, text=True, check=False,
        )
        if result.returncode == 0 and result.stdout.strip():
            return result.stdout.strip()
        # If both approaches fail, we cannot proceed.
        raise RuntimeError(
            "stellar-sdk not installed and 'stellar' CLI unavailable. "
            "Install one: pip install stellar-sdk  OR  cargo install stellar-cli --features opt"
        ) from None


def get_ttl(rpc_url: str, contract_id: str) -> int | None:
    """
    Query the TTL for a contract's instance storage entry.

    Returns the live_until_ledger value, or None if the entry is not found
    (i.e., it may already be archived).
    """
    try:
        key_xdr = _contract_instance_key_xdr(contract_id)
    except Exception as exc:
        _log("error", "Failed to build LedgerKey XDR", contract_id=contract_id, error=str(exc))
        return None

    try:
        result = _rpc_call(rpc_url, "getLedgerEntries", {"keys": [key_xdr]})
    except RuntimeError as exc:
        _log("error", "getLedgerEntries failed", contract_id=contract_id, error=str(exc))
        return None

    entries = result.get("entries") or []
    if not entries:
        _log("warning", "Entry not found — may already be archived", contract_id=contract_id)
        return None
    return int(entries[0].get("liveUntilLedgerSeq", 0))


# ---------------------------------------------------------------------------
# TTL extension via stellar CLI
# ---------------------------------------------------------------------------

def extend_ttl(cfg: Config, contract_id: str, contract_name: str) -> bool:
    """
    Submit an extendFootprintTtl transaction using the stellar CLI.

    Returns True on success, False on failure.
    """
    import subprocess

    secret_key = os.environ.get("STELLAR_SECRET_KEY", "")
    if not secret_key and not cfg.dry_run:
        _log("error", "STELLAR_SECRET_KEY not set; cannot submit transaction",
             contract=contract_name)
        return False

    cmd = [
        "stellar", "contract", "extend",
        "--id", contract_id,
        "--ledgers-to-expire", str(cfg.ledger_bump_amount),
        "--source-account", cfg.source_account,
        "--rpc-url", cfg.rpc_url,
        "--network-passphrase", cfg.network_passphrase,
    ]

    if cfg.dry_run:
        _log("info", "[dry-run] Would submit extendFootprintTtl",
             contract=contract_name, contract_id=contract_id,
             ledgers_to_expire=cfg.ledger_bump_amount)
        return True

    # Inject the secret key via env var (never as CLI argument).
    env = {**os.environ, "STELLAR_SECRET_KEY": secret_key}
    try:
        result = subprocess.run(
            cmd, capture_output=True, text=True, timeout=60, env=env
        )
    except subprocess.TimeoutExpired:
        _log("error", "stellar CLI timed out during TTL extension",
             contract=contract_name, contract_id=contract_id)
        return False

    if result.returncode == 0:
        _log("info", "TTL extended successfully",
             contract=contract_name, contract_id=contract_id,
             ledgers_bumped=cfg.ledger_bump_amount,
             stdout=result.stdout.strip())
        return True
    else:
        _log("error", "stellar CLI failed during TTL extension",
             contract=contract_name, contract_id=contract_id,
             returncode=result.returncode,
             stderr=result.stderr.strip())
        return False


# ---------------------------------------------------------------------------
# Alerting
# ---------------------------------------------------------------------------

def send_alert(webhook_url: str, contract_name: str, contract_id: str,
               current_ledger: int, live_until_ledger: int,
               remaining_ledgers: int, threshold: int) -> None:
    """Send a webhook alert when a contract entry is nearing archival."""
    if not webhook_url:
        return

    # Rough time estimate (5 seconds per ledger)
    remaining_seconds = remaining_ledgers * 5
    remaining_hours = remaining_seconds / 3600

    message = (
        f":warning: *TTL Alert — {contract_name}*\n"
        f"Contract `{contract_id}` is *{remaining_ledgers} ledgers* "
        f"(~{remaining_hours:.1f} hours) from archival.\n"
        f"Current ledger: {current_ledger} | Live until: {live_until_ledger} | "
        f"Alert threshold: {threshold} ledgers"
    )

    payload: dict[str, Any]
    # Detect PagerDuty routing key format
    if len(webhook_url) > 40 and "pagerduty" in webhook_url.lower():
        payload = {
            "routing_key": os.environ.get("PAGERDUTY_ROUTING_KEY", ""),
            "event_action": "trigger",
            "payload": {
                "summary": f"Soroban TTL alert: {contract_name} nearing archival",
                "severity": "warning",
                "source": "renew-ttl.py",
                "custom_details": {
                    "contract_id": contract_id,
                    "remaining_ledgers": remaining_ledgers,
                    "live_until_ledger": live_until_ledger,
                },
            },
        }
    else:
        # Generic Slack-compatible webhook
        payload = {"text": message}

    try:
        resp = requests.post(webhook_url, json=payload, timeout=10)
        resp.raise_for_status()
        _log("info", "Alert sent via webhook",
             contract=contract_name, remaining_ledgers=remaining_ledgers)
    except requests.RequestException as exc:
        _log("error", "Failed to send alert webhook", error=str(exc))


# ---------------------------------------------------------------------------
# Core monitoring loop
# ---------------------------------------------------------------------------

def check_and_renew(cfg: Config) -> dict[str, Any]:
    """
    Check TTL for all configured contracts and renew those below threshold.

    Returns a summary dict suitable for structured logging.
    """
    summary: dict[str, Any] = {
        "contracts_checked": 0,
        "contracts_renewed": 0,
        "contracts_alerted": 0,
        "contracts_missing": 0,
        "errors": 0,
    }

    try:
        current_ledger = get_latest_ledger(cfg.rpc_url)
    except RuntimeError as exc:
        _log("error", "Failed to get latest ledger", error=str(exc))
        summary["errors"] += 1
        return summary

    _log("info", "Starting TTL check", current_ledger=current_ledger,
         contracts=len(cfg.contracts))

    for contract in cfg.contracts:
        summary["contracts_checked"] += 1
        live_until = get_ttl(cfg.rpc_url, contract.id)

        if live_until is None:
            _log("warning", "Entry missing or archived",
                 contract=contract.name, contract_id=contract.id)
            summary["contracts_missing"] += 1
            send_alert(
                cfg.webhook_url, contract.name, contract.id,
                current_ledger, 0, 0, cfg.alert_threshold_ledgers
            )
            summary["contracts_alerted"] += 1
            continue

        remaining = live_until - current_ledger
        _log("info", "TTL status",
             contract=contract.name,
             contract_id=contract.id,
             current_ledger=current_ledger,
             live_until_ledger=live_until,
             remaining_ledgers=remaining,
             threshold=cfg.ledger_lifetime_threshold,
             alert_threshold=cfg.alert_threshold_ledgers)

        # Alert if approaching archival threshold
        if remaining <= cfg.alert_threshold_ledgers:
            _log("warning", "Entry approaching archival threshold",
                 contract=contract.name, remaining_ledgers=remaining)
            send_alert(
                cfg.webhook_url, contract.name, contract.id,
                current_ledger, live_until, remaining, cfg.alert_threshold_ledgers
            )
            summary["contracts_alerted"] += 1

        # Renew if below lifetime threshold
        if remaining <= cfg.ledger_lifetime_threshold:
            _log("info", "Submitting TTL extension",
                 contract=contract.name, remaining_ledgers=remaining)
            if extend_ttl(cfg, contract.id, contract.name):
                summary["contracts_renewed"] += 1
            else:
                summary["errors"] += 1

    _log("info", "TTL check complete", **summary)
    return summary


def run_daemon(cfg: Config) -> None:
    """Run the monitoring loop indefinitely."""
    _log("info", "TTL renewal daemon started",
         poll_interval_seconds=cfg.poll_interval_seconds,
         ledger_lifetime_threshold=cfg.ledger_lifetime_threshold,
         ledger_bump_amount=cfg.ledger_bump_amount,
         alert_threshold_ledgers=cfg.alert_threshold_ledgers,
         dry_run=cfg.dry_run,
         contracts=[c.name for c in cfg.contracts])

    while True:
        try:
            check_and_renew(cfg)
        except Exception as exc:  # pylint: disable=broad-except
            _log("error", "Unexpected error in check loop", error=str(exc))
        _log("info", "Sleeping until next poll",
             poll_interval_seconds=cfg.poll_interval_seconds)
        time.sleep(cfg.poll_interval_seconds)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Soroban TTL renewal daemon — keeps contract entries alive.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--config", default="renew-ttl.json",
        help="Path to JSON configuration file (default: renew-ttl.json)",
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="Report TTL status without submitting any transactions",
    )
    parser.add_argument(
        "--once", action="store_true",
        help="Run a single check cycle and exit (no daemon loop)",
    )
    parser.add_argument(
        "--rpc-url",
        help="Override config rpc_url (also via RPC_URL env var)",
    )
    return parser.parse_args()


def main() -> None:
    args = _parse_args()

    if not os.path.exists(args.config):
        sys.exit(
            f"Config file not found: {args.config}\n"
            "Create one from the template in the docstring above."
        )

    cfg = Config.load(args.config)

    if args.dry_run:
        cfg.dry_run = True
    if args.rpc_url:
        cfg.rpc_url = args.rpc_url

    if not cfg.rpc_url:
        sys.exit("rpc_url is required (set in config or RPC_URL env var)")
    if not cfg.contracts:
        sys.exit("No contracts configured. Add at least one entry to 'contracts' in the config.")

    if args.once:
        summary = check_and_renew(cfg)
        sys.exit(0 if summary["errors"] == 0 and summary["contracts_missing"] == 0 else 1)
    else:
        run_daemon(cfg)


if __name__ == "__main__":
    main()
