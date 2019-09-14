use dag_cbor_derive::DagCbor;
use libipld::codec::cbor::{ReadCbor, WriteCbor};
use libipld::{ipld, Codec, DagCborCodec, Result};

#[derive(Clone, Debug, Default, PartialEq, DagCbor)]
#[ipld(repr = "list")]
struct ListRepr {
    a: bool,
    b: bool,
}

#[derive(Clone, Debug, PartialEq, DagCbor)]
#[ipld(repr = "kinded")]
enum KindedRepr {
    A(bool),
    B { a: u32 },
}

macro_rules! test_case {
    ($data:expr, $ty:ty, $ipld:expr) => {
        let mut bytes = Vec::new();
        $data.write_cbor(&mut bytes)?;
        let ipld = DagCborCodec::decode(&bytes)?;
        assert_eq!(ipld, $ipld);
        let data = <$ty>::read_cbor(&mut bytes.as_slice())?;
        assert_eq!(data, $data);
    };
}

fn main() -> Result<()> {
    test_case! {
        ListRepr::default(),
        ListRepr,
        ipld!([false, false])
    }

    test_case! {
        KindedRepr::A(true),
        KindedRepr,
        ipld!([true])
    }

    test_case! {
        KindedRepr::B { a: 42 },
        KindedRepr,
        ipld!({ "a": 42 })
    }

    Ok(())
}
