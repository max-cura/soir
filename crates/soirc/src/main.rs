use tracing_subscriber::EnvFilter;
use soir::soir::untyped;

fn main() {
    color_eyre::install().unwrap();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
}

static TPCH_1 : &str = r#"
    select
        l_returnflag,
        l_linestatus,
        sum(l_quantity) as sum_qty,
        sum(l_extendedprice) as sum_base_price,
        sum(l_extendedprice * (1 - l_discount)) as sum_disc_price,
        sum(l_extendedprice * (1 - l_discount) * (1 + l_tax)) as sum_charge,
        avg(l_quantity) as avg_qty,
        avg(l_extendedprice) as avg_price,
        avg(l_discount) as avg_disc,
        count(*) as count_order
    from
        lineitem
    where
        l_shipdate <= '1998-09-16'
    group by
        l_returnflag,
        l_linestatus
    order by
        l_returnflag,
        l_linestatus;
    "#;
