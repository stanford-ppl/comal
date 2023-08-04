use std::{fs, path::Path};

use comal::config::Data;
use comal::templates::utils::read_inputs;
use dam_rs::simulation::Program;

#[test]
fn test_matmul_proto() {
    let test_name = "mat_elemadd";
    let filename = home::home_dir().unwrap().join("sam_config.toml");
    let contents = fs::read_to_string(filename).unwrap();
    let data: Data = toml::from_str(&contents).unwrap();
    let formatted_dir = data.sam_config.sam_path;
    let base_path = Path::new(&formatted_dir).join(&test_name);
    let b0_seg_filename = base_path.join("tensor_B_mode_0_seg");
    let b0_crd_filename = base_path.join("tensor_B_mode_0_crd");
    let b1_seg_filename = base_path.join("tensor_B_mode_1_seg");
    let b1_crd_filename = base_path.join("tensor_B_mode_1_crd");
    let b_vals_filename = base_path.join("tensor_B_mode_vals");
    let c0_seg_filename = base_path.join("tensor_C_mode_0_seg");
    let c0_crd_filename = base_path.join("tensor_C_mode_0_crd");
    let c1_seg_filename = base_path.join("tensor_C_mode_1_seg");
    let c1_crd_filename = base_path.join("tensor_C_mode_1_crd");
    let c_vals_filename = base_path.join("tensor_C_mode_vals");

    let _b0_seg = read_inputs::<u32>(&b0_seg_filename);
    let _b0_crd = read_inputs::<u32>(&b0_crd_filename);
    let _b1_seg = read_inputs::<u32>(&b1_seg_filename);
    let _b1_crd = read_inputs::<u32>(&b1_crd_filename);
    let _b_vals = read_inputs::<f32>(&b_vals_filename);
    let _c0_seg = read_inputs::<u32>(&c0_seg_filename);
    let _c0_crd = read_inputs::<u32>(&c0_crd_filename);
    let _c1_seg = read_inputs::<u32>(&c1_seg_filename);
    let _c1_crd = read_inputs::<u32>(&c1_crd_filename);
    let _c_vals = read_inputs::<f32>(&c_vals_filename);

    let _chan_size = 32784;

    let _parent = Program::default();
}
