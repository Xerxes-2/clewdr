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
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            panic!()
        }
    }
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
        #[sea_orm(nullable)]
        pub supports_claude_1m: Option<bool>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub total_input_tokens: Option<i64>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub total_output_tokens: Option<i64>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub window_input_tokens: Option<i64>,
        #[sea_orm(column_type = "BigInteger", nullable)]
        pub window_output_tokens: Option<i64>,
    }
    #[derive(Copy, Clone, Debug, EnumIter)]
    pub enum Relation {}
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            panic!()
        }
    }
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
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            panic!()
        }
    }
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
    impl RelationTrait for Relation {
        fn def(&self) -> RelationDef {
            panic!()
        }
    }
    impl ActiveModelBehavior for ActiveModel {}
}

// Convenient aliases to match previous names used in code
pub use entity_config::{
    ActiveModel as ActiveModelConfig, Column as ColumnConfig, Entity as EntityConfig,
};
pub use entity_cookie::{
    ActiveModel as ActiveModelCookie, Column as ColumnCookie, Entity as EntityCookie,
};
pub use entity_key::{
    ActiveModel as ActiveModelKeyRow, Column as ColumnKeyRow, Entity as EntityKeyRow,
};
pub use entity_wasted::{
    ActiveModel as ActiveModelWasted, Column as ColumnWasted, Entity as EntityWasted,
};
