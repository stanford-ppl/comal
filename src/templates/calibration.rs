#![allow(dead_code)]

use std::fs;
use comal::config::rd_scanner::CalibrationData;

use argparse::{ArgumentParser, Store};
use comal::{
    templates::{
        primitive::Token,
        rd_scanner::{CompressedCrdRdScan, RdScanData},
    },
    token_vec,
};
use dam::{
    simulation::{InitializationOptionsBuilder, ProgramBuilder, RunMode, RunOptionsBuilder},
    utility_contexts::{ConsumerContext, GeneratorContext},
};

use comal::config::rd_scanner::CompressedCrdRdScanConfig;

fn main() {
    let mut config_file = "".to_string();

    {
        let mut ap = ArgumentParser::new();
        ap.refer(&mut config_file).add_option(
            &["--config_file", "-c"],
            Store,
            "Path to config file with autocalibration parameters",
        );
        ap.parse_args_or_exit();
    }

    let contents = fs::read_to_string(config_file).unwrap();
    let calibration_data: CalibrationData = toml::from_str(&contents).unwrap();
    let config = calibration_data.calibration_params;

    rdscanner_test(config);
}

fn rdscanner_test(config: CompressedCrdRdScanConfig) {
    let seg_arr1 = vec![0u32, 3, 6];
    let crd_arr1 = vec![0, 2, 3, 4, 5, 6];
    let in_ref1 = || token_vec!(u32; u32; 0, "N", "S0", "D").into_iter();

    let seg_arr2 = vec![0, 10, 20, 30, 40, 50, 60, 70];
    let crd_arr2 = vec![
        5, 6, 7, 10, 12, 21, 23, 25, 27, 32, 0, 1, 4, 5, 8, 10, 13, 24, 27, 33, 0, 4, 5, 8, 11, 12,
        17, 19, 24, 33, 2, 6, 10, 15, 22, 23, 25, 26, 30, 33, 3, 5, 8, 12, 19, 23, 24, 26, 27, 30,
        0, 1, 2, 6, 17, 22, 23, 24, 25, 33, 0, 2, 5, 7, 12, 13, 20, 25, 27, 30,
    ];
    let in_ref2 = || token_vec!(u32; u32; "N", 1, 2, 1, "S0", "D").into_iter();

    let seg_arr3 = vec![0, 16, 32, 48, 64, 80, 96];
    let crd_arr3 = vec![
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11,
        12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6,
        7, 8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 0,
        1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
    ];
    let in_ref3 =
        || token_vec!(u32; u32; "S0", 2, "N", "N", "S0", "S0", 5, 1, 2, "S1", "D").into_iter();

    let seg_arr4 = vec![0, 55, 110];
    let crd_arr4 = vec![
        0, 1, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 20, 21, 22, 23, 24, 25, 26, 27,
        28, 29, 30, 31, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 44, 45, 46, 47, 48, 50, 51, 52, 53,
        54, 55, 56, 57, 59, 60, 61, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 15, 16, 17, 18, 19, 20,
        21, 22, 23, 24, 25, 26, 27, 29, 30, 31, 32, 33, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45,
        46, 48, 49, 50, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61,
    ];
    let in_ref4 = || {
        token_vec!(u32; u32; 0, 1, 1, 0, "N", 1, 0, "N", "S0", 1, 0, "N", 0, 1, 0, 1, "N", "S1", "D").into_iter()
    };

    let seg_arr5 = vec![0, 13, 26, 39, 52, 65, 78];
    let crd_arr5 = vec![
        20, 21, 22, 23, 24, 25, 26, 27, 29, 210, 211, 212, 214, 20, 22, 23, 24, 25, 26, 27, 28, 29,
        210, 212, 213, 214, 0, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 0, 1, 2, 3, 5, 7, 8, 9, 10,
        11, 12, 13, 14, 0, 1, 2, 3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3, 4, 6, 7, 8, 9, 10,
        12, 13, 14,
    ];
    // empty_root_seq_d
    let in_ref5 = || token_vec!(u32; u32; 0, "S1", "D").into_iter();

    let seg_arr6 = vec![0, 3, 6];
    let crd_arr6 = vec![0, 2, 3, 4, 5, 6];
    let in_ref6 = || token_vec!(u32; u32; 0, "S0", 1, "S1", "D").into_iter();

    let seg_arr7 = vec![0, 13, 26, 39, 52, 65, 78];
    let crd_arr7 = vec![
        0, 1, 2, 3, 4, 5, 6, 7, 9, 10, 11, 12, 14, 0, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 13, 14, 0, 2,
        3, 4, 5, 6, 7, 8, 9, 11, 12, 13, 14, 0, 1, 2, 3, 5, 7, 8, 9, 10, 11, 12, 13, 14, 0, 1, 2,
        3, 5, 6, 7, 8, 9, 10, 11, 12, 13, 0, 1, 2, 3, 4, 6, 7, 8, 9, 10, 12, 13, 14,
    ];
    let in_ref7 = || token_vec!(u32; u32; "S0", 5, 5, 0, "S0", 3, 1, "S1", "D").into_iter();

    let seg_arr8 = vec![0, 199];
    let crd_arr8 = vec![
        2, 3, 4, 24, 29, 31, 34, 37, 49, 57, 60, 61, 67, 68, 70, 72, 86, 91, 100, 101, 102, 110,
        111, 113, 115, 116, 119, 123, 124, 127, 131, 133, 146, 150, 152, 155, 159, 160, 163, 164,
        165, 167, 168, 170, 171, 173, 174, 175, 176, 177, 180, 191, 193, 195, 200, 202, 206, 208,
        210, 217, 218, 219, 221, 224, 225, 230, 231, 234, 239, 240, 246, 248, 249, 253, 254, 257,
        260, 266, 268, 276, 277, 278, 279, 280, 292, 297, 311, 314, 320, 321, 322, 326, 329, 330,
        331, 332, 336, 338, 339, 340, 342, 343, 344, 345, 346, 348, 351, 358, 363, 365, 376, 377,
        380, 381, 396, 399, 403, 408, 411, 414, 415, 416, 417, 423, 424, 428, 429, 433, 442, 444,
        452, 454, 455, 459, 460, 461, 462, 465, 469, 470, 473, 475, 477, 478, 479, 484, 486, 489,
        490, 491, 493, 495, 500, 501, 503, 513, 514, 517, 518, 525, 528, 532, 535, 542, 545, 548,
        550, 557, 560, 563, 564, 565, 569, 570, 574, 576, 578, 580, 583, 585, 589, 592, 595, 597,
        600, 601, 613, 614, 615, 617, 619, 620, 624, 625, 627, 628, 632, 650, 663,
    ];
    let in_ref8 = || token_vec!(u32; u32; "N", 0, 0, "S0", "D").into_iter();

    let seg_arr9 = vec![0, 200];
    let crd_arr9 = vec![
        1, 3, 4, 6, 11, 14, 17, 19, 21, 22, 24, 27, 28, 30, 31, 33, 36, 37, 42, 45, 46, 51, 52, 53,
        55, 57, 61, 62, 67, 68, 69, 70, 71, 73, 74, 78, 79, 80, 85, 86, 91, 93, 95, 96, 97, 100,
        101, 103, 106, 108, 109, 110, 112, 113, 116, 122, 124, 127, 131, 133, 139, 140, 144, 145,
        148, 151, 152, 154, 157, 159, 161, 162, 164, 165, 168, 169, 170, 173, 174, 176, 178, 180,
        183, 184, 185, 186, 187, 188, 191, 192, 194, 195, 198, 199, 201, 203, 206, 211, 213, 214,
        215, 218, 221, 222, 223, 224, 225, 227, 231, 235, 243, 245, 246, 247, 248, 253, 255, 256,
        257, 258, 259, 260, 262, 263, 264, 265, 266, 267, 268, 269, 271, 272, 274, 279, 283, 284,
        286, 291, 293, 299, 300, 302, 304, 305, 306, 307, 308, 309, 310, 311, 313, 314, 315, 316,
        317, 318, 319, 320, 321, 322, 323, 324, 325, 326, 329, 330, 331, 332, 335, 336, 337, 339,
        341, 343, 345, 346, 347, 348, 351, 352, 354, 356, 359, 364, 365, 367, 368, 369, 376, 377,
        378, 379, 382, 383, 385, 387, 388, 390, 392, 393,
    ];
    let in_ref9 = || token_vec!(u32; u32; "N", "N", 0, "S0", "D").into_iter();

    let seg_arr10 = vec![0, 200];
    let crd_arr10 = vec![
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47,
        48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59, 60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70,
        71, 72, 73, 74, 75, 76, 77, 78, 79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93,
        94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112,
        113, 114, 115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129, 130,
        131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142, 143, 144, 145, 146, 147, 148,
        149, 150, 151, 152, 153, 154, 155, 156, 157, 158, 159, 160, 161, 162, 163, 164, 165, 166,
        167, 168, 169, 170, 171, 172, 173, 174, 175, 176, 177, 178, 179, 180, 181, 182, 183, 184,
        185, 186, 187, 188, 189, 190, 191, 192, 193, 194, 195, 196, 197, 198, 199,
    ];
    let in_ref10 = || token_vec!(u32; u32; "N", 0, "N", "S0", "D").into_iter();

    compressed_rd_scan_calibration(seg_arr1, crd_arr1, in_ref1, config.clone());
    compressed_rd_scan_calibration(seg_arr2, crd_arr2, in_ref2, config.clone());
    compressed_rd_scan_calibration(seg_arr3, crd_arr3, in_ref3, config.clone());
    compressed_rd_scan_calibration(seg_arr4, crd_arr4, in_ref4, config.clone());
    compressed_rd_scan_calibration(seg_arr5, crd_arr5, in_ref5, config.clone());
    compressed_rd_scan_calibration(seg_arr6, crd_arr6, in_ref6, config.clone());
    compressed_rd_scan_calibration(seg_arr7, crd_arr7, in_ref7, config.clone());
    compressed_rd_scan_calibration(seg_arr8, crd_arr8, in_ref8, config.clone());
    compressed_rd_scan_calibration(seg_arr9, crd_arr9, in_ref9, config.clone());
    compressed_rd_scan_calibration(seg_arr10, crd_arr10, in_ref10, config.clone());
}

fn compressed_rd_scan_calibration<IRT>(
    seg_arr: Vec<u32>,
    crd_arr: Vec<u32>,
    in_ref: fn() -> IRT,
    config: CompressedCrdRdScanConfig,
) where
    IRT: Iterator<Item = Token<u32, u32>> + 'static,
{
    let mut parent = ProgramBuilder::default();
    let (ref_sender, ref_receiver) = parent.unbounded::<Token<u32, u32>>();
    let (crd_sender, crd_receiver) = parent.unbounded::<Token<u32, u32>>();
    let (in_ref_sender, in_ref_receiver) = parent.unbounded::<Token<u32, u32>>();
    let data = RdScanData::<u32, u32> {
        in_ref: in_ref_receiver,
        out_ref: ref_sender,
        out_crd: crd_sender,
    };
    let mut cr = CompressedCrdRdScan::new(data, seg_arr, crd_arr);
    cr.set_timings(config);
    let gen1 = GeneratorContext::new(in_ref, in_ref_sender);
    let crd_checker = ConsumerContext::new(crd_receiver);
    let ref_checker = ConsumerContext::new(ref_receiver);

    parent.add_child(gen1);
    parent.add_child(crd_checker);
    parent.add_child(ref_checker);
    parent.add_child(cr);

    let initialized = parent
        .initialize(
            InitializationOptionsBuilder::default()
                .run_flavor_inference(true)
                .build()
                .unwrap(),
        )
        .unwrap();

    let executed = initialized.run(
        RunOptionsBuilder::default()
            .mode(RunMode::Simple)
            .build()
            .unwrap(),
    );

    // let diff: i64 =
    //     TryInto::<i64>::try_into(executed.elapsed_cycles().unwrap()).unwrap() - actual_cycle;

    // Return simulated cycles
    let sim_cycles = executed.elapsed_cycles().unwrap();

    println!("Simulated cycles: {:?}", sim_cycles);
}
