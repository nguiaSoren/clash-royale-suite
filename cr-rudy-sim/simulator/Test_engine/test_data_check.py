"""
Quick diagnostic: what characters are loaded? Is golemite among them?

Run: python test_data_check.py
"""
import cr_engine

data = cr_engine.load_data("data/")

print("\n=== All loaded character keys ===")
keys = sorted(data.character_keys())
for k in keys:
    stats = data.get_character_stats(k)
    print(f"  {k:25s}  elixir={stats['elixir']}  hp={stats['hitpoints']}")

print(f"\nTotal: {len(keys)} characters")

# Check for common sub-troops that are needed for death spawns
sub_troops = ["golemite", "lava_pup", "skeleton", "bat", "elixir_blob"]
print("\n=== Sub-troop availability (needed for death spawns) ===")
for st in sub_troops:
    found = st in keys or st.lower() in [k.lower() for k in keys]
    print(f"  {st:20s}  {'✓ loaded' if found else '✗ MISSING'}")

print("\n=== Building keys ===")
bkeys = sorted(data.building_keys())
for k in bkeys:
    print(f"  {k}")
print(f"Total: {len(bkeys)} buildings")