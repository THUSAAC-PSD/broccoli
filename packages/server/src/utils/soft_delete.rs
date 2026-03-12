use sea_orm::{ColumnTrait, EntityTrait, PrimaryKeyTrait, QueryFilter, Select};

/// Marks an entity as soft-deletable and provides scoped query constructors.
///
/// Implement this trait on the entity's `Entity` type to get `find_active()` and
/// `find_active_by_id()` helpers that automatically filter out deleted rows
/// (`deleted_at IS NULL`), keeping individual handlers free of boilerplate.
pub trait SoftDeletable: EntityTrait {
    /// The concrete `Column` variant that holds the soft-deletion timestamp.
    type DeletedAtColumn: ColumnTrait;

    /// Returns the `deleted_at` column for this entity.
    fn deleted_at() -> Self::DeletedAtColumn;

    /// `Entity::find()` pre-filtered to non-deleted rows.
    fn find_active() -> Select<Self> {
        Self::find().filter(Self::deleted_at().is_null())
    }

    /// `Entity::find_by_id()` pre-filtered to non-deleted rows.
    fn find_active_by_id<V>(values: V) -> Select<Self>
    where
        V: Into<<Self::PrimaryKey as PrimaryKeyTrait>::ValueType>,
    {
        Self::find_by_id(values).filter(Self::deleted_at().is_null())
    }
}
