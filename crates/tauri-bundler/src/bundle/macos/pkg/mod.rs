// Copyright 2016-2019 Cargo-Bundle developers <https://github.com/burtonageo/cargo-bundle>
// Copyright 2019-2024 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

use super::app;
use crate::{
  bundle::{settings::Arch, Bundle},
  utils::CommandExt,
  PackageType, Settings,
};

use std::{fs, path::PathBuf, process::Command};

pub struct Bundled {
  pub pkg: Vec<PathBuf>,
  pub app: Vec<PathBuf>,
}

/// Bundles the project into a macOS PKG installer.
/// Returns a vector of PathBuf that shows where the PKG was created.
pub fn bundle_project(settings: &Settings, bundles: &[Bundle]) -> crate::Result<Bundled> {
  // generate the .app bundle if needed
  let app_bundle_paths = if !bundles
    .iter()
    .any(|bundle| bundle.package_type == PackageType::MacOsBundle)
  {
    app::bundle_project(settings)?
  } else {
    Vec::new()
  };

  // get the target path
  let output_path = settings.project_out_directory().join("bundle/macos");
  let pkg_output_path = output_path.parent().unwrap().join("pkg");

  fs::create_dir_all(&pkg_output_path)?;

  let package_base_name = format!(
    "{}_{}_{}",
    settings.product_name(),
    settings.version_string(),
    match settings.binary_arch() {
      Arch::X86_64 => "x64",
      Arch::AArch64 => "aarch64",
      Arch::Universal => "universal",
      target => {
        return Err(crate::Error::ArchError(format!(
          "Unsupported architecture: {target:?}"
        )));
      }
    }
  );

  let pkg_name = format!("{}.pkg", &package_base_name);
  let pkg_path = pkg_output_path.join(&pkg_name);

  let product_name = settings.product_name();
  let bundle_file_name = format!("{product_name}.app");
  let app_bundle_path = output_path.join(&bundle_file_name);

  log::info!(action = "Bundling"; "{} ({})", pkg_name, pkg_path.display());

  // Step 1: Create a component package using pkgbuild
  // This packages the .app bundle into a component package
  let main_component_filename = settings
    .macos()
    .pkg_main_component
    .as_ref()
    .and_then(|c| c.filename.clone())
    .unwrap_or_else(|| format!("{}.pkg", product_name));
  let component_pkg_path = pkg_output_path.join(&main_component_filename);

  let mut pkgbuild_cmd = Command::new("pkgbuild");
  pkgbuild_cmd
    .arg("--component")
    .arg(&app_bundle_path)
    .arg("--install-location")
    .arg("/Applications");

  // Add scripts if provided for main component
  if let Some(main_component) = &settings.macos().pkg_main_component {
    if let Some(scripts_path) = &main_component.scripts {
      pkgbuild_cmd.arg("--scripts").arg(scripts_path);
    }
  }

  pkgbuild_cmd.arg(&component_pkg_path);

  log::info!(action = "Running"; "pkgbuild (component package)");
  pkgbuild_cmd
    .output_ok()
    .map_err(|e| crate::Error::ShellScriptError(format!("pkgbuild failed: {}", e)))?;

  // Step 1.5: Build extra component packages
  for extra_component in &settings.macos().pkg_extra_components {
    let extra_component_path = pkg_output_path.join(&extra_component.filename);

    let mut extra_pkgbuild_cmd = Command::new("pkgbuild");

    // Add common arguments
    extra_pkgbuild_cmd
      .arg("--identifier")
      .arg(&extra_component.identifier)
      .arg("--version")
      .arg(&extra_component.version);

    // Handle different package modes
    if extra_component.nopayload {
      // Virtual package mode (no payload, scripts only)
      extra_pkgbuild_cmd.arg("--nopayload");
    } else if let Some(component_path) = &extra_component.component {
      // Component mode (bundle or app)
      extra_pkgbuild_cmd.arg("--component").arg(component_path);

      if let Some(install_location) = &extra_component.install_location {
        extra_pkgbuild_cmd
          .arg("--install-location")
          .arg(install_location);
      }
    } else if let Some(root_path) = &extra_component.root {
      // Root mode (directory of files)
      extra_pkgbuild_cmd.arg("--root").arg(root_path);

      if let Some(install_location) = &extra_component.install_location {
        extra_pkgbuild_cmd
          .arg("--install-location")
          .arg(install_location);
      }
    } else {
      return Err(crate::Error::GenericError(format!(
        "Extra component '{}' must specify either nopayload=true, component, or root",
        extra_component.identifier
      )));
    }

    // Add scripts if provided
    if let Some(scripts_path) = &extra_component.scripts {
      extra_pkgbuild_cmd.arg("--scripts").arg(scripts_path);
    }

    // Add output path
    extra_pkgbuild_cmd.arg(&extra_component_path);

    log::info!(action = "Running"; "pkgbuild (extra component: {})", extra_component.identifier);
    extra_pkgbuild_cmd.output_ok().map_err(|e| {
      crate::Error::ShellScriptError(format!(
        "pkgbuild failed for component '{}': {}",
        extra_component.identifier, e
      ))
    })?;
  }

  // Step 2: Read distribution.xml
  // Use configured path or default to distribution.xml
  let distribution_xml_path = if let Some(custom_path) = &settings.macos().pkg_distribution {
    std::env::current_dir()?.join(custom_path)
  } else {
    std::env::current_dir()?.join("distribution.xml")
  };

  if !distribution_xml_path.exists() {
    return Err(crate::Error::GenericError(format!(
      "distribution.xml not found at {}. PKG bundling requires a distribution.xml file.",
      distribution_xml_path.display()
    )));
  }

  log::info!(action = "Using"; "distribution.xml from {}", distribution_xml_path.display());

  // Step 3: Create the distribution package using productbuild
  // This combines the component package(s) into a final installer
  let mut productbuild_cmd = Command::new("productbuild");
  productbuild_cmd
    .arg("--distribution")
    .arg(&distribution_xml_path)
    .arg("--package-path")
    .arg(&pkg_output_path)
    .arg(&pkg_path);

  log::info!(action = "Running"; "productbuild (distribution package)");
  productbuild_cmd
    .output_ok()
    .map_err(|e| crate::Error::ShellScriptError(format!("productbuild failed: {}", e)))?;

  // Sign PKG if needed
  if let Some(pkg_sign_command) = &settings.macos().pkg_sign_command {
    // Use custom signing command
    super::sign::sign_pkg_custom(&pkg_path, pkg_sign_command)?;
  } else {
    // Use native productsign
    let identity = settings.macos().signing_identity.as_deref();
    if identity != Some("-") {
      if let Some(identity) = identity {
        super::sign::sign_pkg(&pkg_path, identity)?;
      }
    }
  }

  // Notarize PKG if custom command is configured
  if let Some(notarize_command) = &settings.macos().pkg_notarize_command {
    super::sign::notarize_custom(&pkg_path, notarize_command)?;
  }

  log::info!(action = "Finished"; "PKG installer at {}", pkg_path.display());

  Ok(Bundled {
    pkg: vec![pkg_path],
    app: app_bundle_paths,
  })
}
