use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::time::{Duration, SystemTime};

use chrono::format::Item;
use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use fastanvil::Region;
use fastnbt::error::Result;
use fastnbt::{ByteArray, Value};
use serde::Deserialize;

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
enum MinecraftVersion {
    V1_4_5,
    V1_4_6,
    V1_4_7,
    V1_5,
    V1_5_1,
    V1_5_2,
    V1_6,
    V1_6_1,
    V1_6_2,
    V1_6_3,
    V1_6_4,
    V1_7,
    V1_7_1,
    V1_7_2,
    V1_7_3,
    V1_7_4,
    V1_7_5,
    V1_7_6,
    V1_7_7,
    V1_7_8,
    V1_7_10,
    V1_8,
    V1_8_1,
    V1_8_2,
    V1_8_3,
    V1_8_4,
    V1_8_5,
    V1_8_6,
    V1_8_7,
    V1_8_8,
    V1_8_9,
    V1_9_4,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ChunkContainer {
    level: HashMap<String, Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Chunk {
    data_version: i32,
}

#[derive(Hash, PartialEq, Eq)]
struct BlockIdData(i32, u8);

impl std::fmt::Debug for BlockIdData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.0, self.1)
    }
}

fn main() {
    let path = std::env::args().nth(1).unwrap();
    dbg!(&path);
    let file = File::open(&path).unwrap();

    let mut region = Region::from_stream(file).unwrap();

    for chunkd in region.iter().flatten() {
        let mut min_version = MinecraftVersion::V1_4_5 as u8;
        let mut max_version = MinecraftVersion::V1_9_4 as u8;

        // std::fs::write("chunk.dat", &chunkd.data).unwrap();

        let chunk: ChunkContainer = fastnbt::from_bytes(&chunkd.data).unwrap();
        let chunk = chunk.level;

        let x = chunk["xPos"].as_i64().unwrap();
        let z = chunk["zPos"].as_i64().unwrap();
        // println!("{{\"x\":{x},\"z\":{z}}},");
        // continue;

        let mut level_key_versions: HashMap<
            &str,
            (Option<MinecraftVersion>, Option<MinecraftVersion>),
        > = HashMap::new();
        level_key_versions.insert(
            "V",
            (Some(MinecraftVersion::V1_7), Some(MinecraftVersion::V1_8_9)),
        );
        level_key_versions.insert("LightPopulated", (Some(MinecraftVersion::V1_7), None));
        level_key_versions.insert("InhabitedTime", (Some(MinecraftVersion::V1_6), None));
        // TileTicks

        for key in chunk.keys() {
            if let Some((min, max)) = level_key_versions.get(key.as_str()) {
                if let Some(min) = min {
                    min_version = min_version.max(*min as _);
                }

                if let Some(max) = max {
                    max_version = max_version.min(*max as _);
                }
            }
        }

        let mut tile_tick_key_versions: HashMap<
            &str,
            (Option<MinecraftVersion>, Option<MinecraftVersion>),
        > = HashMap::new();
        tile_tick_key_versions.insert("BlockLight", (Some(MinecraftVersion::V1_4_6), None));

        if let Some(tile_ticks) = chunk.get("TileTicks").and_then(|v| v.as_list()) {
            match tile_ticks[0].as_compound().unwrap()["i"] {
                Value::Int(_) => max_version = max_version.min(MinecraftVersion::V1_7_10 as _),
                Value::String(_) => min_version = min_version.max(MinecraftVersion::V1_8 as _),
                _ => {}
            }
        }

        let mut unique_block_iddata = HashSet::new();
        for section in chunk["Sections"].as_list().unwrap() {
            let section = section.as_compound().unwrap();

            let blocks = section["Blocks"]
                .as_byte_array()
                .unwrap()
                .clone()
                .into_inner();
            let mut blocks =
                unsafe { std::mem::transmute::<&[i8], &[u8]>(blocks.as_slice()) }.iter();

            let data = section["Data"]
                .as_byte_array()
                .unwrap()
                .clone()
                .into_inner();
            let mut data = NibbleIter::from_iter(
                unsafe { std::mem::transmute::<Vec<i8>, Vec<u8>>(data) }.into_iter(),
            );

            let add = section.get("Add").and_then(|a| a.as_byte_array());
            if let Some(add) = add {
                let add = add.clone().into_inner();
                let mut add = NibbleIter::from_iter(
                    unsafe { std::mem::transmute::<Vec<i8>, Vec<u8>>(add) }.into_iter(),
                );

                while let (Some(id), Some(data), Some(add)) =
                    (blocks.next(), data.next(), add.next())
                {
                    unique_block_iddata
                        .insert(BlockIdData((*id as i32) | ((add as i32) << 8), data));
                }
            } else {
                while let (Some(id), Some(data)) = (blocks.next(), data.next()) {
                    unique_block_iddata.insert(BlockIdData(*id as i32, data));
                }
            }
        }

        for block in &unique_block_iddata {
            let i = block.0;
            let d = block.1;

            // 1.4.6
            // nether brick slab (double, lower, upper)
            if (i == 43 && d == 6) || (i == 44 && (d == 6 || d == 14)) {
                min_version = min_version.max(MinecraftVersion::V1_4_6 as _);
            }

            // 1.5
            if i == 146 // trapped chest
            || i == 147 || i == 148 // weighted pressure plate (light, heavy)
            || i == 149 || i == 150 // redstone comparator (unpowered, powered)
            || i == 151 // daylight detector
            || i == 152 // block of redstone
            || i == 153 // nether quartz ore
            || i == 154 // hopper
            || i == 155 // block of quartz
            || i == 156 // quartz stairs
            || i == 157 // activator rail
            || i == 158 // dropper
            // quartz slab (double, lower, upper)
            || (i == 43 && d == 7) || (i == 44 && (d == 7 || d == 15))
            {
                min_version = min_version.max(MinecraftVersion::V1_5 as _);
            }

            // 1.6
            if i == 159 // stained clay
            || i == 170 // hay bale
            || i == 171 // carpet
            || i == 172 // hardened clay
            || i == 173 // block of coal
            || 1 == 2
            {
                min_version = min_version.max(MinecraftVersion::V1_6 as _);
            }

            // 1.7
            if (i == 3 && (d == 1 || d == 2)) // coarse dirt & podzol
            || (i == 6 && (d == 4 || d == 5)) // saplings (acacia & roofed_oak)
            || (i == 12 && (d == 1)) // red sand
            // stained glass replaced locked chest
            || i == 161 // leaves2
            || i == 162 // log2
            || i == 174 // packed ice
            || i == 175 // double plant
            // todo: new flowers & infested blocks
            || 1 == 2
            {
                min_version = min_version.max(MinecraftVersion::V1_7 as _);
            }

            // todo: 1.7.1+
        }

        if min_version >= (MinecraftVersion::V1_7 as _) {
            println!("provider.getOrGenerateChunk({x}, {z});");
        }

        // println!(
        //     "{:?} - {x},{z} - from {:?} to {:?}",
        //     path,
        //     unsafe { std::mem::transmute::<u8, MinecraftVersion>(min_version) },
        //     unsafe { std::mem::transmute::<u8, MinecraftVersion>(max_version) }
        // );
        // println!(
        //     "{:?} to {:?}",
        //     unsafe { std::mem::transmute::<u8, MinecraftVersion>(min_version) },
        //     unsafe { std::mem::transmute::<u8, MinecraftVersion>(max_version) }
        // );
        // println!("{:?}", unique_block_iddata);

        if 1 != 2 {
            // panic!()
        }
    }
}

trait NbtValueExt {
    fn as_list(&self) -> Option<&[Value]>;
    fn as_compound(&self) -> Option<&HashMap<String, Value>>;
    fn as_byte_array(&self) -> Option<&ByteArray>;
}

impl NbtValueExt for Value {
    fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(list) => Some(list),
            _ => None,
        }
    }

    fn as_compound(&self) -> Option<&HashMap<String, Value>> {
        match self {
            Value::Compound(compound) => Some(compound),
            _ => None,
        }
    }

    fn as_byte_array(&self) -> Option<&ByteArray> {
        match self {
            Value::ByteArray(byte_array) => Some(byte_array),
            _ => None,
        }
    }
}

struct NibbleIter<I>
where
    I: Iterator<Item = u8>,
{
    inner: I,
    last: u8,
    last_lsb: bool,
}

impl<I> NibbleIter<I>
where
    I: Iterator<Item = u8>,
{
    fn from_iter(inner: I) -> Self {
        Self {
            inner,
            last: 0,
            last_lsb: false,
        }
    }
}

impl<I> Iterator for NibbleIter<I>
where
    I: Iterator<Item = u8>,
{
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        let value = if !self.last_lsb {
            self.last = self.inner.next()?;
            self.last & 0xf
        } else {
            self.last >> 4
        };

        self.last_lsb = !self.last_lsb;
        Some(value)
    }
}
