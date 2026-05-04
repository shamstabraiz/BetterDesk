use anyhow::{Context, Result};
use keyring::{Entry, Error as KeyringError};

const SERVICE: &str = "Yomie";
const ACCESS_TOKEN_ACCOUNT: &str = "access_token";
const DEVICE_PASSWORD_ACCOUNT: &str = "device_password";

fn entry(account: &str) -> Result<Entry> {
	Entry::new(SERVICE, account).context("failed to create secure storage entry")
}

fn load_secret(account: &str) -> Result<Option<String>> {
	let entry = entry(account)?;
	match entry.get_password() {
		Ok(value) if value.is_empty() => Ok(None),
		Ok(value) => Ok(Some(value)),
		Err(KeyringError::NoEntry) => Ok(None),
		Err(err) => Err(err).context("failed to load secret from secure storage"),
	}
}

fn store_secret(account: &str, value: Option<&str>) -> Result<()> {
	let entry = entry(account)?;
	match value.filter(|v| !v.is_empty()) {
		Some(secret) => entry
			.set_password(secret)
			.context("failed to save secret to secure storage"),
		None => match entry.delete_credential() {
			Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
			Err(err) => Err(err).context("failed to delete secret from secure storage"),
		},
	}
}

pub fn load_access_token() -> Result<Option<String>> {
	load_secret(ACCESS_TOKEN_ACCOUNT)
}

pub fn store_access_token(value: Option<&str>) -> Result<()> {
	store_secret(ACCESS_TOKEN_ACCOUNT, value)
}

pub fn load_device_password() -> Result<Option<String>> {
	load_secret(DEVICE_PASSWORD_ACCOUNT)
}

pub fn store_device_password(value: Option<&str>) -> Result<()> {
	store_secret(DEVICE_PASSWORD_ACCOUNT, value)
}