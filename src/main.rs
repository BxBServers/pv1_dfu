#![allow(dead_code)]

use std::fs::{self, File};
use std::{collections::HashMap, path::PathBuf};

use clap::Parser;
use color_eyre::eyre::Context;
use fastanvil::Region;
use fastnbt::Value;
use tracing::warn;

mod ids;

#[derive(Debug, Parser)]
struct Args {
    input_world_path: PathBuf,
    output_world_path: PathBuf,
}

fn main() -> color_eyre::Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let region_dir_paths = ["region/", "DIM-1/region/", "DIM1/region/"]
        .iter()
        .map(|sub_path| {
            (
                args.input_world_path.join(sub_path),
                args.output_world_path.join(sub_path),
            )
        });

    for (input_region_dir_path, output_region_dir_path) in region_dir_paths {
        match fs::read_dir(&input_region_dir_path) {
            Ok(region_paths) => {
                fs::create_dir_all(&output_region_dir_path).wrap_err_with(|| {
                    format!(
                        "Failed to create output directory at {:?}.",
                        &output_region_dir_path
                    )
                })?;

                for region_path in region_paths {
                    let region_path = region_path?;

                    let input_region_file = File::open(region_path.path())?;
                    let output_region_file = File::options()
                        .read(true)
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(output_region_dir_path.join(region_path.file_name()))?;

                    let input_region = Region::from_stream(input_region_file)?;
                    let output_region = Region::new(output_region_file)?;

                    downgrade_region(input_region, output_region)?;
                }
            }
            Err(err) => {
                warn!(
                    "Failed to read region directory: {:?} - {}, continuing...",
                    input_region_dir_path, err
                );
            }
        }
    }

    Ok(())
}

fn downgrade_region(mut input: Region<File>, mut output: Region<File>) -> color_eyre::Result<()> {
    for chunk_data in input.iter() {
        let chunk_data = chunk_data?;
        let mut chunk: HashMap<String, Value> = fastnbt::from_bytes(&chunk_data.data)?;

        let level = chunk.get_mut("Level").unwrap().as_compound().unwrap();

        // remove new fields
        level.remove("V"); // 1.7
        level.remove("InhabitedTime"); // 1.8
        level.remove("LightPopulated"); // 1.8

        downgrade_entities(level.get_mut("Entities").unwrap().as_list().unwrap())?;
        downgrade_tile_entities(level.get_mut("TileEntities").unwrap().as_list().unwrap())?;

        let tile_ticks = level.get_mut("TileTicks").and_then(|t| t.as_list());
        if let Some(tile_ticks) = tile_ticks {
            for tile_tick in tile_ticks {
                let tile_tick = tile_tick.as_compound().unwrap();

                if let Value::String(id) = &tile_tick["i"] {
                    tile_tick.insert("i".into(), Value::Int(ids::new_to_old(id).unwrap() as _));
                }
            }
        }

        output.write_chunk(
            chunk_data.x,
            chunk_data.z,
            fastnbt::to_bytes(&chunk)?.as_ref(),
        )?;
    }

    Ok(())
}

fn downgrade_entities(entities: &mut Vec<Value>) -> color_eyre::Result<()> {
    for entity in entities {
        let entity = entity.as_compound().unwrap();
        let id = entity["id"].as_str().unwrap().to_string();

        if id == "Item" {
            entity.remove("PickupDelay"); // 1.8
            entity.remove("Thrower"); // 1.7
            entity.remove("Owner"); // 1.7

            downgrade_item_stack(entity.get_mut("Item").unwrap().as_compound().unwrap())?;
        }

        // decoration entity (painting & item frame) direction (1.8)
        if let Some(facing) = entity.get("Facing") {
            let facing = facing.as_i64().unwrap() as i8;
            entity.remove("Facing");
            entity.insert("Direction".into(), Value::Byte(facing));

            match facing {
                0 => {
                    let z = entity.remove("TileZ").unwrap().as_i64().unwrap() as i32;
                    entity.insert("TileZ".into(), Value::Int(z - 1));
                }
                1 => {
                    let x = entity.remove("TileX").unwrap().as_i64().unwrap() as i32;
                    entity.insert("TileX".into(), Value::Int(x - -1));
                }
                2 => {
                    let z = entity.remove("TileZ").unwrap().as_i64().unwrap() as i32;
                    entity.insert("TileZ".into(), Value::Int(z - -1));
                }
                3 => {
                    let x = entity.remove("TileX").unwrap().as_i64().unwrap() as i32;
                    entity.insert("TileX".into(), Value::Int(x - 1));
                }

                _ => {}
            }

            // item frame rotation (1.8)
            if let Some(Value::Byte(item_rotation)) = entity.get_mut("ItemRotation") {
                *item_rotation /= 2;
            }
        }

        if id == "Arrow" {
            entity.remove("life"); // 1.7

            // 1.8
            if let Value::String(in_tile) = &entity["inTile"] {
                entity.insert(
                    "inTile".into(),
                    Value::Byte(ids::new_to_old(in_tile).unwrap() as i8),
                );
            }
        }

        // throwable entities
        if id == "Snowball"
            || id == "ThrownEnderpearl"
            || id == "ThrownPotion"
            || id == "ThrownExpBottle"
        {
            // 1.8
            if let Value::String(id) = &entity["inTile"] {
                entity.insert(
                    "inTile".into(),
                    Value::Byte(ids::new_to_old(id).unwrap() as i8),
                );
            }
        }

        // explosive projectile entities
        if id == "Fireball" || id == "SmallFireball" || id == "WitherSkull" {
            // 1.8
            if let Value::String(id) = &entity["inTile"] {
                entity.insert(
                    "inTile".into(),
                    Value::Byte(ids::new_to_old(id).unwrap() as i8),
                );
            }
        }

        if id == "ThrownPotion" {
            if let Some(potion) = entity.get_mut("Potion") {
                downgrade_item_stack(potion.as_compound().unwrap())?;
            }
        }

        if id == "ItemFrame" {
            if let Some(item) = entity.get_mut("Item") {
                downgrade_item_stack(item.as_compound().unwrap())?;
            }
        }

        if id == "FallingSand" {
            // 1.8
            if let Some(Value::String(id)) = entity.remove("Block") {
                entity.insert(
                    "Tile".into(),
                    Value::Byte(ids::new_to_old(&id).unwrap() as i8),
                );
            }

            // 1.5
            entity.remove("TileEntityData");
        }

        if id == "FireworksRocketEntity" {
            downgrade_item_stack(
                entity
                    .get_mut("FireworksItem")
                    .unwrap()
                    .as_compound()
                    .unwrap(),
            )?;
        }

        // unify minecarts (1.5)
        if id == "MinecartRideable" {
            entity.insert("id".into(), Value::String("Minecart".into()));
            entity.insert("Type".into(), Value::Int(0));
        }

        if id == "MinecartChest" {
            entity.insert("id".into(), Value::String("Minecart".into()));
            entity.insert("Type".into(), Value::Int(1));

            let items = entity.get_mut("Items").unwrap().as_list().unwrap();
            for item in items {
                downgrade_item_stack(item.as_compound().unwrap())?;
            }
        }

        if id == "MinecartFurnace" {
            entity.insert("id".into(), Value::String("Minecart".into()));
            entity.insert("Type".into(), Value::Int(2));
        }

        // mob entities
        if id == "Mob"
            || id == "Monster"
            || id == "Creeper"
            || id == "Skeleton"
            || id == "Spider"
            || id == "Giant"
            || id == "Zombie"
            || id == "Slime"
            || id == "Ghast"
            || id == "PigZombie"
            || id == "Enderman"
            || id == "CaveSpider"
            || id == "Silverfish"
            || id == "Blaze"
            || id == "LavaSlime"
            || id == "EnderDragon"
            || id == "WitherBoss"
            || id == "Bat"
            || id == "Witch"
            || id == "Pig"
            || id == "Sheep"
            || id == "Cow"
            || id == "Chicken"
            || id == "Squid"
            || id == "Wolf"
            || id == "MushroomCow"
            || id == "SnowMan"
            || id == "Ozelot"
            || id == "VillagerGolem"
            || id == "Villager"
        {
            // living entity
            entity.remove("HurtByTimestamp"); // 1.8
            entity.remove("HealF"); // 1.6
            entity.remove("Attributes"); // 1.6
            entity.remove("AbsorptionAmount"); // 1.6

            // 1.8
            if let Some(active_effects) = entity.get_mut("ActiveEffects").and_then(|e| e.as_list())
            {
                for effect in active_effects {
                    effect.as_compound().unwrap().remove("ShowParticles");
                }
            }

            // mob entity
            for equipment in entity.get_mut("Equipment").unwrap().as_list().unwrap() {
                downgrade_item_stack(equipment.as_compound().unwrap())?;
            }

            // 1.6
            entity.remove("Leashed"); // 1.6
            entity.remove("Leash"); // 1.6
            entity.remove("NoAI"); // 1.8
        }

        if id == "Zombie" || id == "PigZombie" {
            entity.remove("CanBreakDoors"); // 1.7
        }

        if id == "Slime" || id == "LavaSlime" {
            entity.remove("wasOnGround"); // 1.8
        }

        if id == "PigZombie" {
            entity.remove("HurtBy"); // 1.8
        }

        // passive entities
        if id == "Pig"
            || id == "Sheep"
            || id == "Cow"
            || id == "Chicken"
            || id == "Wolf"
            || id == "MushroomCow"
            || id == "Ozelot"
            || id == "Villager"
        {
            entity.remove("ForcedAge"); // 1.8
        }

        if id == "Chicken" {
            entity.remove("EggLayTime"); // 1.8
            entity.remove("IsChickenJockey"); // 1.7.3 (1.7.5 kinda)
        }

        // tameable entities
        if id == "Wolf" || id == "Ozelot" {
            if let Some(owner_id) = entity.remove("OwnerUUID") {
                entity.insert("Owner".into(), owner_id);
            }
        }

        if id == "Villager" {
            entity.remove("Career"); // 1.8
            entity.remove("CareerLevel"); // 1.8
            entity.remove("Willing"); // 1.8
            entity.remove("Inventory"); // 1.8

            if let Some(offers) = entity.get_mut("Offers").and_then(NbtValueExt::as_compound) {
                let recipes = offers.get_mut("Recipes").unwrap().as_list().unwrap();
                for recipe in recipes {
                    let recipe = recipe.as_compound().unwrap();

                    recipe.remove("rewardExp"); // 1.8
                    downgrade_item_stack(recipe.get_mut("buy").unwrap().as_compound().unwrap())?;
                    downgrade_item_stack(recipe.get_mut("sell").unwrap().as_compound().unwrap())?;

                    if let Some(buy2) = recipe.get_mut("buy") {
                        downgrade_item_stack(buy2.as_compound().unwrap())?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn downgrade_tile_entities(tile_entities: &mut Vec<Value>) -> color_eyre::Result<()> {
    for tile_entity in tile_entities {
        let tile_entity = tile_entity.as_compound().unwrap();
        let id = tile_entity["id"].as_str().unwrap().to_string();

        tile_entity.remove("CustomName"); // 1.5

        // inventories
        if id == "Furnace" || id == "Chest" || id == "Trap" || id == "Cauldron" {
            let items = tile_entity.get_mut("Items").unwrap().as_list().unwrap();
            for item in items {
                downgrade_item_stack(item.as_compound().unwrap())?;
            }
        }

        if id == "Furnace" {
            tile_entity.remove("CookTimeTotal"); // 1.8
        }

        if id == "RecordPlayer" {
            if let Some(item) = tile_entity
                .get_mut("RecordItem")
                .and_then(NbtValueExt::as_compound)
            {
                downgrade_item_stack(item)?;
            }
        }

        // lockable container
        if id == "Trap" || id == "Cauldron" {
            tile_entity.remove("Lock"); // 1.8
        }

        if id == "Sign" {
            downgrade_sign_text(tile_entity, "Text1")?;
            downgrade_sign_text(tile_entity, "Text2")?;
            downgrade_sign_text(tile_entity, "Text3")?;
            downgrade_sign_text(tile_entity, "Text4")?;
        }

        if id == "Control" {
            tile_entity.remove("SuccessCount"); // 1.7
            tile_entity.remove("TrackOutput"); // 1.7
            tile_entity.remove("LastOutput"); // 1.7
            tile_entity.remove("CommandStats"); // 1.7
        }

        if id == "Skull" {
            tile_entity.remove("Owner"); // 1.7.6
            tile_entity.remove("ExtraType"); // 1.7
        }

        // should probably do mob spawners, but uh
        // no
    }

    Ok(())
}

fn downgrade_sign_text(
    sign: &mut HashMap<String, Value>,
    text_key: &str,
) -> color_eyre::Result<()> {
    let text = sign
        .remove(text_key)
        .and_then(|value| value.as_str().map(|s| s.to_owned()))
        .unwrap_or("".to_owned());

    let text = serde_json::from_str::<serde_json::Value>(&text)
        .map(|v| match v {
            serde_json::Value::Null => "".to_string(),
            serde_json::Value::String(string) => string,
            serde_json::Value::Number(number) => number.to_string(),

            other => unimplemented!("sign text: {other:?}"),
        })
        .unwrap_or(text);

    sign.insert(text_key.into(), Value::String(text));

    Ok(())
}

fn downgrade_item_stack(item_stack: &mut HashMap<String, Value>) -> color_eyre::Result<()> {
    // 1.8
    if let Some(Value::String(ident)) = item_stack.get("id") {
        item_stack.insert("id".into(), Value::Short(ids::new_to_old(ident).unwrap()));
    }

    // maybe some things on tag im missing
    // let tag = item_stack.get_mut("tag").unwrap().as_compound().unwrap();

    Ok(())
}

trait NbtValueExt {
    fn as_list(&mut self) -> Option<&mut Vec<Value>>;
    fn as_compound(&mut self) -> Option<&mut HashMap<String, Value>>;
}

impl NbtValueExt for Value {
    fn as_list(&mut self) -> Option<&mut Vec<Value>> {
        match self {
            Value::List(list) => Some(list),
            _ => None,
        }
    }

    fn as_compound(&mut self) -> Option<&mut HashMap<String, Value>> {
        match self {
            Value::Compound(compound) => Some(compound),
            _ => None,
        }
    }
}
