use sea_orm::entity::prelude::*;

pub mod entity_config {
    use super::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "clewdr_config")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub k: String,
        pub data: String,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub updated_at: Option<i64>,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod entity_cookie {
    use super::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "cookies")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub cookie: String,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub reset_time: Option<i64>,
        #[sea_orm(nullable)]
        pub token_access: Option<String>,
        #[sea_orm(nullable)]
        pub token_refresh: Option<String>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub token_expires_at: Option<i64>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub token_expires_in: Option<i64>,
        #[sea_orm(nullable)]
        pub token_org_uuid: Option<String>,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod entity_wasted {
    use super::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "wasted_cookies")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub cookie: String,
        pub reason: String,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
    impl ActiveModelBehavior for ActiveModel {}
}

pub mod entity_key {
    use super::*;
    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[sea_orm(table_name = "keys")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub key: String,
        pub count_403: i64,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation { fn def(&self) -> RelationDef { panic!() } }
    impl ActiveModelBehavior for ActiveModel {}
}

// Convenient aliases to match previous names used in code
pub use entity_config::{Entity as EntityConfig, Column as ColumnConfig, ActiveModel as ActiveModelConfig};
pub use entity_cookie::{Entity as EntityCookie, Column as ColumnCookie, ActiveModel as ActiveModelCookie};
pub use entity_wasted::{Entity as EntityWasted, Column as ColumnWasted, ActiveModel as ActiveModelWasted};
pub use entity_key::{Entity as EntityKeyRow, Column as ColumnKeyRow, ActiveModel as ActiveModelKeyRow};

