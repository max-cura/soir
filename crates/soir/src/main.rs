use lasso::Rodeo;
use crate::soir::{Id, StrId};

pub mod soir;

fn main() {
    use soir::untyped::{Expr, Expr::*};
    let mut rodeo = Rodeo::new();
    let id = |n: &str| Id(rodeo.get_or_intern(n));
    let b = Box::new;

    let app_v = |l: &str, r: Vec<Expr>| {
        Apply(b(Var(id(l))), r)
    };
    let app_id = |l: &str, r: Vec<&str>| {
        app_v(l, r.into_iter().map(|s| Var(id(s))).collect())
    };
    let mut s_rodeo = Rodeo::new();
    let sid = |n: &str| StrId(rodeo.get_or_intern(n));

    // let f4 = Let(
    //     id("f4"),
    //     b(Lambda(vec![id("a0")], b(app_v(
    //         "Z_Tuple2_cons",
    //         vec![
    //             app_id("U_1_returnflag", vec!["a0"]),
    //             app_id("U_1_linestatus", vec!["a0"]),
    //         ]
    //         )))),
    //     b(app_id("Z_Stream_sortBy", vec!["s3", "f4"])),
    // );
    let s3 = app_id("Z_Stream_map", vec!["s2", "f3"]);
    let f3 = Let(
        id("f3"),
        b(Lambda(vec![id("a0")], b(
            Let(id("t0"), b(app_id("Z_Tuple2_0", vec!["a0"])), b(
                Let(id("t1"), b(app_id("Z_Tuple2_1", vec!["a0"])), b(app_v("U_1_cons", vec![
                    app_id("U_2_returnflag", vec!["t0"]),
                    app_id("U_2_linestatus", vec!["t0"]),
                    app_id("U_0_sum_qty", vec!["t1"])
                ])))
            ))
        ))),
        b(s3)
    );
    let s2 = Let(
        id("s2"),
        b(app_id("Z_Stream_groupAggregation", vec!["s1", "f1", "f2"])),
        b(f3)
    );
    let f2 = Let(
        id("f2"),
        b(Lambda(vec![id("a0"),id("a1")], b(app_v("U_0_cons", vec![
            app_v("Z_int_+", vec![
                app_id("U_0_sum_qty", vec!["a0"]),
                app_id("T_lineitem_quantity", vec!["a1"]),
            ])
        ])))),
        b(s2),
    );
    let f1 = Let(
        id("f1"),
        b(Lambda(vec![id("a0")], b(app_v("Z_Tuple2_cons", vec![
            app_id("T_lineitem_returnflag", vec!["a0"]),
            app_id("T_lineitem_linestatus", vec!["a0"]),
        ])))),
        b(f2),
    );
    let s1 = Let(
        id("s1"),
        b(app_id("Z_Stream_filter", vec!["s0", "f0"])),
        b(f1),
    );
    let f0 = Let(
        id("f0"),
        b(Lambda(vec![id("a0")], b(app_v("Z_Date_lte", vec![
            app_id("T_lineitem_shipdate", vec!["a0"]),
            Constant(soir::Constant::Str(sid("1998-09-16"))),
        ])))),
        b(s1),
    );
    let s0 = Let(
        id("s0"),
        b(app_v("T_lineitem_Data_streamAll", vec![Constant(soir::Constant::Unit)])),
        b(f0),
    );
}

#[used]
static X : &str =
    r#"
let s0 = Data.streamAll T_lineitem
let f0 = \row -> let t0 = row:shipdate in Date.lte t0 '1998-09-16'
let s1 = S.filter s0 f0
let f1 = \row -> let t0 = row:returnflag in let t1 = row:linestatus in (t0, t1)
let f2 = \agg,row ->
    { sum_qty = agg:sum_qty + row:quantity
 (* , sum_base_price = agg:sum_base_price + row:extendedprice
    , sum_disc_price = agg:sum_disc_price + (row:extendedprice * (1 - row:discount))
    , sum_charge = agg:sum_charge + (row:extendedprice * (1 - row:discount) * (1 + row:tax))
    , avg_qty$sum = agg:avg_qty$sum + row:quantity
    , avg_qty$count = agg:avg_qty$count + 1
    , avg_price$sum = agg:avg_price$sum + row:extendedprice
    , avg_price$count = agg:avg_price$count + 1
    , avg_disc$sum = agg:avg_disc$sum + row:discount
    , avg_disc$count = agg:avg_disc$count + 1
    , count_order = agg:count_order + 1 *)
    }
let s2 = S.groupAggregation s1 f1 f2
let f3 = \(group,agg) ->
    { returnflag = group:returnflag
    , linestatus = group:linestatus
    , sum_qty = agg:sum_qty
 (* , sum_base_price = agg:sum_base_price
    , sum_disc_price = agg:sum_disc_price
    , sum_charge = agg:sum_charge
    , avg_qty = agg:avg_qty$sum / agg:avg_qty$count
    , avg_price =  agg:avg_price$sum / agg:avg_price$count
    , avg_disc = agg:avg_disc$sum / agg:avg_disc$count
    , count_order = agg:count_order *)
    }
S.map s2 f3
(*
let f4 = \r0,r1 -> match (cmp r0:returnflag r1:returnflag) in
    Lt -> Lt,
    Gt -> Gt,
    Eq -> cmp r0:linestatus r1:linestatus
S.sortBy s3 f4
*)
"#;
