package dev.horizonlab.parity;

import java.io.PrintWriter;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryDataLoader;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.Bootstrap;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.MultiPackResourceManager;
import net.minecraft.tags.TagLoader;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.item.enchantment.ConditionalEffect;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentEffectComponents;
import net.minecraft.world.item.enchantment.effects.EnchantmentLocationBasedEffect;
import net.minecraft.world.item.enchantment.effects.ReplaceDisk;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/**
 * Original oracle: runs the vanilla frost_walker enchantment's replace_disk
 * (ReplaceDisk.apply, reached through the enchantment's location_changed effects
 * and its codec-loaded predicate) on a synthetic level. The level answers
 * getBlockState and setBlockAndUpdate from the case's cells, and getEntities
 * from the case's blocking boxes, so the vanilla isUnobstructed path runs.
 *
 * Each input line is "LEVEL POSITION DEFAULT CELLS ENTITIES" (see
 * crates/horizon-client/examples/probe/frost_walker_fixture.rs); each output
 * line is "INDEX x,y,z,EVENT;..." for the cells the level set, in order.
 */
public final class VanillaFrostWalkerOracle {
    static final class Stop extends RuntimeException {}

    static Map<Long, BlockState> cells = new HashMap<>();
    static BlockState fallback;
    static List<AABB> boxes = new ArrayList<>();
    static List<Entity> blockers = new ArrayList<>();
    static List<String> placed = new ArrayList<>();

    static long key(BlockPos p) {
        return ((long) p.getX() & 0x3FFFFFFL) << 38 | ((long) p.getZ() & 0x3FFFFFFL) << 12 | ((long) p.getY() & 0xFFFL);
    }

    static final class World extends ServerLevel {
        World() {
            super(null, null, null, null, null, null, false, 0L, List.of(), false);
        }

        @Override
        public BlockState getBlockState(BlockPos pos) {
            return cells.getOrDefault(key(pos), fallback);
        }

        @Override
        public boolean setBlockAndUpdate(BlockPos pos, BlockState state) {
            BlockState old = getBlockState(pos);
            if (old.equals(state)) {
                return false;
            }
            cells.put(key(pos), state);
            return true;
        }

        @Override
        public void gameEvent(Entity entity, net.minecraft.core.Holder<GameEvent> event, BlockPos pos) {
            String id = event.unwrapKey().orElseThrow().identifier().toString();
            placed.add(pos.getX() + "," + pos.getY() + "," + pos.getZ() + "," + id);
        }

        @Override
        public List<Entity> getEntities(Entity except, AABB bb, java.util.function.Predicate<? super Entity> selector) {
            List<Entity> found = new ArrayList<>();
            for (Entity e : blockers) {
                if (selector.test(e)) {
                    found.add(e);
                }
            }
            return found;
        }
    }

    static Field field(Class<?> c, String name) throws Exception {
        Field f = c.getDeclaredField(name);
        f.setAccessible(true);
        return f;
    }

    static BlockState state(String kind) {
        return switch (kind) {
            case "air" -> Blocks.AIR.defaultBlockState();
            case "cave_air" -> Blocks.CAVE_AIR.defaultBlockState();
            case "void_air" -> Blocks.VOID_AIR.defaultBlockState();
            case "water" -> Blocks.WATER.defaultBlockState();
            case "flowing" -> Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL, 1);
            case "stone" -> Blocks.STONE.defaultBlockState();
            case "ice" -> Blocks.ICE.defaultBlockState();
            default -> throw new IllegalArgumentException(kind);
        };
    }

    static double bits(String token) {
        return Double.longBitsToDouble(Long.parseUnsignedLong(token, 16));
    }

    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var repo = ServerPacksSource.createVanillaTrustedRepository();
        repo.reload();
        repo.setSelected(List.of("vanilla"));
        var resources = new MultiPackResourceManager(PackType.SERVER_DATA, repo.openAllSelected());
        var layers = RegistryLayer.createRegistryAccess();
        var staticTags = TagLoader.loadTagsForExistingRegistries(resources, layers.getLayer(RegistryLayer.STATIC));
        var context = TagLoader.buildUpdatedLookups(layers.getAccessForLoading(RegistryLayer.WORLD), staticTags);
        RegistryAccess.Frozen world = RegistryDataLoader.load(resources, context, RegistryDataLoader.WORLD_REGISTRIES, Runnable::run).join();
        staticTags.forEach(Registry.PendingTags::apply);
        RegistryAccess access = layers.replaceFrom(RegistryLayer.WORLD, world).compositeAccess();
        net.minecraft.core.registries.BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(access).forEach(p -> p.apply());

        Enchantment frost = world.lookupOrThrow(Registries.ENCHANTMENT)
            .get(ResourceKey.create(Registries.ENCHANTMENT, Identifier.parse("minecraft:frost_walker")))
            .orElseThrow()
            .value();
        ReplaceDisk disk = null;
        for (ConditionalEffect<EnchantmentLocationBasedEffect> effect : frost.getEffects(EnchantmentEffectComponents.LOCATION_CHANGED)) {
            if (effect.effect() instanceof ReplaceDisk replace) {
                disk = replace;
            }
        }
        if (disk == null) {
            throw new IllegalStateException("frost_walker has no replace_disk");
        }

        var unsafe = field(Class.forName("sun.misc.Unsafe"), "theUnsafe").get(null);
        var allocate = Class.forName("sun.misc.Unsafe").getMethod("allocateInstance", Class.class);
        Field bb = field(Entity.class, "bb");
        Field blocksBuilding = field(Entity.class, "blocksBuilding");
        Field random = field(Entity.class, "random");
        World level = (World) allocate.invoke(unsafe, World.class);
        ArmorStand actor = (ArmorStand) allocate.invoke(unsafe, ArmorStand.class);
        random.set(actor, RandomSource.create(0L));

        try (var out = new PrintWriter(Files.newBufferedWriter(Path.of(args[1]), StandardCharsets.UTF_8));
             var lines = Files.lines(Path.of(args[0]), StandardCharsets.UTF_8)) {
            int index = 0;
            for (String line : (Iterable<String>) lines::iterator) {
                String[] f = line.split(" ");
                int enchantmentLevel = Integer.parseInt(f[0]);
                String[] p = f[1].split(",");
                Vec3 position = new Vec3(bits(p[0]), bits(p[1]), bits(p[2]));
                fallback = state(f[2]);
                cells.clear();
                if (!f[3].equals("-")) {
                    for (String cell : f[3].split(";")) {
                        String[] c = cell.split(",");
                        cells.put(key(new BlockPos(Integer.parseInt(c[0]), Integer.parseInt(c[1]), Integer.parseInt(c[2]))), state(c[3]));
                    }
                }
                blockers.clear();
                if (!f[4].equals("-")) {
                    for (String entry : f[4].split(";")) {
                        String[] b = entry.split(",");
                        AABB box = new AABB(bits(b[0]), bits(b[1]), bits(b[2]), bits(b[3]), bits(b[4]), bits(b[5]));
                        ArmorStand blocker = (ArmorStand) allocate.invoke(unsafe, ArmorStand.class);
                        bb.set(blocker, box);
                        blocksBuilding.setBoolean(blocker, true);
                        blockers.add(blocker);
                    }
                }
                placed.clear();
                disk.apply(level, enchantmentLevel, null, actor, position);
                out.println(index + " " + (placed.isEmpty() ? "-" : String.join(";", placed)));
                index++;
            }
        }
    }
}
