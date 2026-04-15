use sea_orm::{ColumnTrait, EntityTrait, PrimaryKeyTrait, QueryFilter, Select};

pub trait SoftDeletable: EntityTrait {
    type DeletedAtColumn: ColumnTrait;

    fn deleted_at() -> Self::DeletedAtColumn;

    fn find_active() -> Select<Self> {
        Self::find().filter(Self::deleted_at().is_null())
    }

    fn find_active_by_id<V>(values: V) -> Select<Self>
    where
        V: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        Self::find_by_id(values).filter(Self::deleted_at().is_null())
    }
}
