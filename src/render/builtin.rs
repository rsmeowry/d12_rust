use crate::vertex_struct;

vertex_struct!(Vertex2 {
    pos: [f32; 3] as POSITION;
    col: [f32; 4] as COLOR;
});

vertex_struct!(Vertex3 {
    pos: [f32; 3] as POSITION;
    norm: [f32; 3] as NORMAL;
    uv: [f32; 2] as TEXCOORD0;
});

