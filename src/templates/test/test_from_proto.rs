#[cfg(test)]
mod tests {

    use std::{fs, path::Path};

    use crate::context::broadcast_context::BroadcastContext;
    use crate::context::generator_context::GeneratorContext;

    use crate::simulation::Program;
    use crate::templates::ops::ALUMulOp;
    use crate::templates::sam::accumulator::{Reduce, ReduceData};
    use crate::templates::sam::alu::make_alu;
    use crate::templates::sam::array::{Array, ArrayData};
    use crate::templates::sam::joiner::{CrdJoinerData, Intersect};
    use crate::templates::sam::primitive::{Repsiggen, Token};
    use crate::templates::sam::rd_scanner::{CompressedCrdRdScan, RdScanData};
    use crate::templates::sam::repeat::{RepSigGenData, Repeat, RepeatData, RepeatSigGen};
    use crate::templates::sam::test::config::Data;
    use crate::templates::sam::utils::read_inputs;
    use crate::templates::sam::wr_scanner::{CompressedWrScan, ValsWrScan};
    use crate::token_vec;

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

        let b0_seg = read_inputs::<u32>(&b0_seg_filename);
        let b0_crd = read_inputs::<u32>(&b0_crd_filename);
        let b1_seg = read_inputs::<u32>(&b1_seg_filename);
        let b1_crd = read_inputs::<u32>(&b1_crd_filename);
        let b_vals = read_inputs::<f32>(&b_vals_filename);
        let c0_seg = read_inputs::<u32>(&c0_seg_filename);
        let c0_crd = read_inputs::<u32>(&c0_crd_filename);
        let c1_seg = read_inputs::<u32>(&c1_seg_filename);
        let c1_crd = read_inputs::<u32>(&c1_crd_filename);
        let c_vals = read_inputs::<f32>(&c_vals_filename);

        let chan_size = 32784;

        let mut parent = Program::default();
    }
}
