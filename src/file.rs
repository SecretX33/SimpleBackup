// use crate::config::TargetConfig;
// use std::path::Path;
// use walkdir::WalkDir;
// 
// pub fn replicate_folder_structure(
//     target_config: &TargetConfig,
//     source_path: &Path,
//     destination_path: &Path,
// ) {
//     let walkdir = walk_folder(&target_config);
// 
//     let valid_entries = walkdir.into_iter()
//         .filter_entry(|entry| entry.file_type().is_file() || entry.file_type().is_dir())
//         .filter_map(|entry| {
//             if let Err(error) = &entry {
//                 log_client.warn(format!("Failed to read entry, skipping: {:?}", error));
//             }
//             entry.ok()
//         });
// 
//     let mut success = 0;
//     let mut failed = 0;
// 
//     for entry in valid_entries {
//         let entry_path = entry.into_path();
//         let relative_path = pathdiff::diff_paths(&entry_path, source_path).unwrap();
//         let destination_path = destination_path.join(relative_path);
// 
//         if entry_path.is_dir() {
//             if let Err(err) = std::fs::create_dir_all(&destination_path) {
//                 log_client.warn(format!("Failed to create directory '{}' (this might cause more errors): {:?}", destination_path.display(), err));
//             };
//             continue;
//         }
// 
//         let resolved_source_path = if entry_path.is_symlink() {
//             let result = entry_path.read_link()
//                 .inspect_err(|err| log_client.warn(format!("Failed to read symlink target, skipping entry: {:?}", err)));
// 
//             if let Ok(value) = result { value } else { continue }
//         } else { entry_path };
// 
//         if create_symlink(&resolved_source_path, &destination_path, log_client) {
//             success += 1;
//         } else {
//             failed += 1;
//         }
//     }
// 
//     log_client.finished(success, failed, cancellation_token.load(Ordering::Relaxed));
// }
// 
// fn walk_folder(config: &TargetConfig) -> WalkDir {
//     let folder = config.path.as_path();
//     let mut walk = WalkDir::new(folder).follow_links(false);
//     if let Some(min_depth) = config.min_depth {
//         walk = walk.min_depth(min_depth);
//     }
//     if let Some(max_depth) = config.max_depth {
//         walk = walk.max_depth(max_depth);
//     }
//     walk
// }