// Automation checks against every imported ReBoneBase animation set:
//   UnrealEditor-Cmd <Project>.uproject -ExecCmds="Automation RunTests ReBoneBase;Quit" -unattended -nullrhi
#include "AssetRegistry/AssetRegistryModule.h"
#include "Components/BoxComponent.h"
#include "Misc/AutomationTest.h"
#include "PaperFlipbook.h"
#include "ReBoneAnimationSet.h"
#include "ReBoneFlipbookComponent.h"

#if WITH_DEV_AUTOMATION_TESTS

namespace ReBoneTests
{
	TArray<UReBoneAnimationSet*> LoadSets()
	{
		IAssetRegistry& Registry = FModuleManager::LoadModuleChecked<FAssetRegistryModule>("AssetRegistry").Get();
		Registry.SearchAllAssets(true);
		TArray<FAssetData> Assets;
		Registry.GetAssetsByClass(UReBoneAnimationSet::StaticClass()->GetClassPathName(), Assets);
		TArray<UReBoneAnimationSet*> Sets;
		for (const FAssetData& Asset : Assets)
		{
			if (UReBoneAnimationSet* Set = Cast<UReBoneAnimationSet>(Asset.GetAsset()))
			{
				Sets.Add(Set);
			}
		}
		return Sets;
	}

	struct FFired
	{
		FString Name;
		int32 Frame;
		bool operator==(const FFired& Other) const { return Name == Other.Name && Frame == Other.Frame; }
	};

	/** Markers expected while playing `Clip` from 0 to `Seconds`, from the exported holds alone. */
	TArray<FFired> Expected(const FReBoneClip& Clip, double Seconds)
	{
		TArray<FFired> Out;
		double Start = 0.0;
		int32 Index = 0;
		while (true)
		{
			for (const FReBoneMarker& Marker : Clip.Frames[Index].Markers)
			{
				Out.Add({Marker.Name, Index});
			}
			const double End = Start + Clip.Frames[Index].DurationMs / 1000.0;
			if (End > Seconds + 1e-6)
			{
				return Out;
			}
			Start = End;
			if (++Index == Clip.Frames.Num())
			{
				if (!Clip.bLooped)
				{
					return Out;
				}
				Index = 0;
			}
		}
	}

	/** Frame shown at `Seconds` according to the exported holds. */
	int32 FrameAt(const FReBoneClip& Clip, double Seconds, double& DistanceToBoundary)
	{
		double Total = 0.0;
		for (const FReBoneFrame& Frame : Clip.Frames)
		{
			Total += Frame.DurationMs / 1000.0;
		}
		double Time = Clip.bLooped ? FMath::Fmod(Seconds, Total) : FMath::Min(Seconds, Total - 1e-9);
		double Start = 0.0;
		for (int32 I = 0; I < Clip.Frames.Num(); ++I)
		{
			const double End = Start + Clip.Frames[I].DurationMs / 1000.0;
			if (Time < End)
			{
				DistanceToBoundary = FMath::Min(Time - Start, End - Time);
				return I;
			}
			Start = End;
		}
		DistanceToBoundary = 0.0;
		return Clip.Frames.Num() - 1;
	}

	UReBoneFlipbookComponent* NewPlayer(UReBoneAnimationSet* Set, TArray<FFired>& Fired)
	{
		UReBoneFlipbookComponent* Player = NewObject<UReBoneFlipbookComponent>(GetTransientPackage());
		Player->AnimationSet = Set;
		Player->OnMarkerNative.AddLambda([&Fired](const FString& Name, const FString&, int32 Frame) { Fired.Add({Name, Frame}); });
		return Player;
	}
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReBoneImportedAssets, "ReBoneBase.ImportedAssets", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReBoneImportedAssets::RunTest(const FString&)
{
	const TArray<UReBoneAnimationSet*> Sets = ReBoneTests::LoadSets();
	TestTrue(TEXT("At least one imported animation set"), Sets.Num() > 0);
	for (const UReBoneAnimationSet* Set : Sets)
	{
		for (const FReBoneClip& Clip : Set->Clips)
		{
			const FString What = Set->GetName() + TEXT(" ") + Clip.Animation;
			if (!TestNotNull(*(What + TEXT(" flipbook")), Clip.Flipbook.Get()))
			{
				continue;
			}
			const UPaperFlipbook* Flipbook = Clip.Flipbook;
			TestEqual(*(What + TEXT(" key frames")), Flipbook->GetNumKeyFrames(), Clip.Frames.Num());
			for (int32 I = 0; I < FMath::Min(Flipbook->GetNumKeyFrames(), Clip.Frames.Num()); ++I)
			{
				const FPaperFlipbookKeyFrame& Key = Flipbook->GetKeyFrameChecked(I);
				TestNotNull(*FString::Printf(TEXT("%s frame %d sprite"), *What, I), Key.Sprite.Get());
				const double Hold = Key.FrameRun / static_cast<double>(Flipbook->GetFramesPerSecond());
				TestEqual(*FString::Printf(TEXT("%s frame %d hold"), *What, I), Hold, Clip.Frames[I].DurationMs / 1000.0, 1e-5);
			}
		}
	}
	return true;
}

IMPLEMENT_SIMPLE_AUTOMATION_TEST(FReBonePlayback, "ReBoneBase.Playback", EAutomationTestFlags::EditorContext | EAutomationTestFlags::ProductFilter)
bool FReBonePlayback::RunTest(const FString&)
{
	using namespace ReBoneTests;
	for (UReBoneAnimationSet* Set : LoadSets())
	{
		for (const FReBoneClip& Clip : Set->Clips)
		{
			if (!Clip.Flipbook || Clip.Frames.Num() == 0)
			{
				continue;
			}
			const FString What = Set->GetName() + TEXT(" ") + Clip.Animation;
			double Total = 0.0;
			for (const FReBoneFrame& Frame : Clip.Frames)
			{
				Total += Frame.DurationMs / 1000.0;
			}
			// Small steps: every frame is shown at the right time with its own regions.
			TArray<FFired> Fired;
			UReBoneFlipbookComponent* Player = NewPlayer(Set, Fired);
			TestTrue(*(What + TEXT(" plays")), Player->PlayAnimation(Clip.Animation));
			TArray<UBoxComponent*> Pool;
			const double Step = 1.0 / 240.0;
			const double Duration = Total * 2.5;
			double Advanced = 0.0;
			for (double Time = 0.0; Time < Duration; Time += Step)
			{
				double Margin = 0.0;
				const int32 Want = FrameAt(Clip, Time, Margin);
				if (Margin > 0.0005)
				{
					TestEqual(*FString::Printf(TEXT("%s frame at %.4f s"), *What, Time), Player->GetCurrentFrameIndex(), Want);
				}
				const FReBoneFrame& Frame = Clip.Frames[Player->GetCurrentFrameIndex()];
				TestEqual(*(What + TEXT(" active regions")), Player->GetActiveRegionCount(), Frame.Regions.Num());
				for (int32 R = 0; R < Frame.Regions.Num() && R < Player->GetRegionBoxes().Num(); ++R)
				{
					const UBoxComponent* Box = Player->GetRegionBoxes()[R];
					const FTransform Local = Box->GetRelativeTransform();
					const FVector Extent = Box->GetUnscaledBoxExtent();
					for (const FVector2D& Corner : Frame.Regions[R].Corners)
					{
						const FVector Want3 = Set->PixelToLocal(Corner);
						bool bMatched = false;
						for (const FVector Sign : {FVector(-1, 0, -1), FVector(1, 0, -1), FVector(1, 0, 1), FVector(-1, 0, 1)})
						{
							bMatched |= Local.TransformPosition(Extent * Sign).Equals(Want3, 0.01);
						}
						TestTrue(*FString::Printf(TEXT("%s region %d corner (%.2f, %.2f)"), *What, R, Corner.X, Corner.Y), bMatched);
					}
				}
				if (Pool.IsEmpty() && Player->GetRegionBoxes().Num() > 0)
				{
					for (UBoxComponent* Box : Player->GetRegionBoxes())
					{
						Pool.Add(Box);
					}
				}
				Player->Advance(Step);
				Advanced += Step;
			}
			// Region boxes are reused, never recreated, after the pool has grown.
			for (int32 I = 0; I < Pool.Num(); ++I)
			{
				TestTrue(*(What + TEXT(" reuses region boxes")), Player->GetRegionBoxes().IsValidIndex(I) && Player->GetRegionBoxes()[I] == Pool[I]);
			}
			TestTrue(*(What + TEXT(" markers in order")), Fired == Expected(Clip, Advanced));

			// One large step still fires every passed frame's markers, in order.
			TArray<FFired> Burst;
			UReBoneFlipbookComponent* Fast = NewPlayer(Set, Burst);
			Fast->PlayAnimation(Clip.Animation);
			Fast->Advance(static_cast<float>(Total * 1.5));
			TestTrue(*(What + TEXT(" markers in one long tick")), Burst == Expected(Clip, Total * 1.5));
			TestEqual(*(What + TEXT(" looping")), Fast->IsPlaying(), Clip.bLooped);

			// Restart returns to frame zero and fires it again.
			Burst.Reset();
			Fast->Restart();
			TestEqual(*(What + TEXT(" restart frame")), Fast->GetCurrentFrameIndex(), 0);
			TestTrue(*(What + TEXT(" restart markers")), Burst == Expected(Clip, 0.0));
		}
		// Changing only the direction keeps frame and time and fires nothing.
		for (const FReBoneClip& Clip : Set->Clips)
		{
			const int32 Other = Set->Clips.IndexOfByPredicate([&](const FReBoneClip& C) { return C.Name == Clip.Name && !C.Direction.IsEmpty() && C.Direction != Clip.Direction; });
			if (Clip.Direction.IsEmpty() || Other == INDEX_NONE || Clip.Frames.Num() < 2)
			{
				continue;
			}
			TArray<FFired> Fired;
			UReBoneFlipbookComponent* Player = NewPlayer(Set, Fired);
			Player->PlayDirection(Clip.Name, Clip.Direction);
			Player->Advance((Clip.Frames[0].DurationMs + Clip.Frames[1].DurationMs * 0.5f) / 1000.f);
			const int32 Frame = Player->GetCurrentFrameIndex();
			const float Position = Player->GetPlaybackPosition();
			Fired.Reset();
			TestTrue(TEXT("direction switch"), Player->PlayDirection(Clip.Name, Set->Clips[Other].Direction));
			TestEqual(TEXT("direction switch animation"), Player->GetCurrentAnimation(), Set->Clips[Other].Animation);
			TestEqual(TEXT("direction switch keeps frame"), Player->GetCurrentFrameIndex(), Frame);
			TestEqual(TEXT("direction switch keeps time"), Player->GetPlaybackPosition(), Position, 1e-4f);
			TestEqual(TEXT("direction switch fires nothing"), Fired.Num(), 0);
			break;
		}
	}
	return true;
}

#endif
