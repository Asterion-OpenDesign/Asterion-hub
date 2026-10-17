//! Facts read from the MSI file itself. The publisher records them in the catalog; the Hub reads them
//! again from the downloaded file and refuses to run a package that says something different. That
//! catches "right hash, wrong product" publishing mistakes and any package that is not an MSI at all.

use crate::error::{HubError, Result};
use crate::manifest::MsiFacts;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MsiProps {
    pub product_name: String,
    pub product_version: String,
    pub upgrade_code: String,
    pub product_code: String,
}

pub fn read_msi(path: &Path) -> Result<MsiProps> {
    let not_msi = |why: String| HubError::UnexpectedPackage(format!("not a readable MSI package: {why}"));
    let file = std::fs::File::open(path).map_err(|e| HubError::io("open package", e))?;
    let mut pkg = msi::Package::open(file).map_err(|e| not_msi(e.to_string()))?;
    let rows = pkg.select_rows(msi::Select::table("Property")).map_err(|e| not_msi(e.to_string()))?;
    let (mut name, mut version, mut upgrade, mut product) = (None, None, None, None);
    for row in rows {
        let (Some(k), Some(v)) = (row["Property"].as_str(), row["Value"].as_str()) else { continue };
        match k {
            "ProductName" => name = Some(v.to_string()),
            "ProductVersion" => version = Some(v.to_string()),
            "UpgradeCode" => upgrade = Some(v.to_string()),
            "ProductCode" => product = Some(v.to_string()),
            _ => {}
        }
    }
    let need = |o: Option<String>, what: &str| o.ok_or_else(|| not_msi(format!("{what} is missing")));
    Ok(MsiProps {
        product_name: need(name, "ProductName")?,
        product_version: need(version, "ProductVersion")?,
        upgrade_code: need(upgrade, "UpgradeCode")?,
        product_code: need(product, "ProductCode")?,
    })
}

/// Compare the file with what the signed catalog promised.
pub fn check_against_catalog(path: &Path, facts: &MsiFacts) -> Result<MsiProps> {
    let p = read_msi(path)?;
    let mismatch = |field: &str, want: &str, got: &str| {
        HubError::UnexpectedPackage(format!("the package {field} is {got:?}, the catalog says {want:?}"))
    };
    if !p.upgrade_code.eq_ignore_ascii_case(&facts.upgrade_code) {
        return Err(mismatch("UpgradeCode", &facts.upgrade_code, &p.upgrade_code));
    }
    if p.product_name != facts.product_name {
        return Err(mismatch("ProductName", &facts.product_name, &p.product_name));
    }
    if p.product_version != facts.product_version {
        return Err(mismatch("ProductVersion", &facts.product_version, &p.product_version));
    }
    Ok(p)
}
