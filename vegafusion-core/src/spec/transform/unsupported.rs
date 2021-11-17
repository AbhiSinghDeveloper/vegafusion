use crate::spec::transform::TransformSpecTrait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

macro_rules! unsupported_transforms {
    ( $( $name:ident ),* ) => {
        $(
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        pub struct $name {
            #[serde(flatten)]
            pub extra: HashMap<String, Value>,
        }

        impl TransformSpecTrait for $name {
            fn supported(&self) -> bool { false }
        }

        )*
    };
}

unsupported_transforms!(
    CountpatternTransformSpec,
    ContourTransformSpec,
    CrossTransformSpec,
    CrossfilterTransformSpec,
    DensityTransformSpec,
    DotbinTransformSpec,
    FlattenTransformSpec,
    FoldTransformSpec,
    ForceTransformSpec,
    GeojsonTransformSpec,
    GeopathTransformSpec,
    GeopointTransformSpec,
    GeoshapeTransformSpec,
    GraticuleTransformSpec,
    HeatmapTransformSpec,
    IdentifierTransformSpec,
    ImputeTransformSpec,
    IsocontourTransformSpec,
    KdeTransformSpec,
    Kde2dTransformSpec,
    LabelTransformSpec,
    LinkpathTransformSpec,
    LoessTransformSpec,
    LookupTransformSpec,
    NestTransformSpec,
    PackTransformSpec,
    PartitionTransformSpec,
    PieTransformSpec,
    PivotTransformSpec,
    ProjectTransformSpec,
    QuantileTransformSpec,
    RegressionTransformSpec,
    ResolvefilterTransformSpec,
    SampleTransformSpec,
    SequenceTransformSpec,
    StackTransformSpec,
    StratifyTransformSpec,
    TreeTransformSpec,
    TreelinksTransformSpec,
    TreemapTransformSpec,
    VoronoiTransformSpec,
    WindowTransformSpec,
    WordcloudTransformSpec
);
