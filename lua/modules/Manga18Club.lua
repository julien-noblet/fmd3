----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = '719b7acfe3d847ab9e6b4f5661d8bbf0'
	m.Name                     = 'Manga18Club'
	m.RootURL                  = 'https://manga18.club'
	m.Category                 = 'Webcomics'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
end

----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

DirectoryPagination = '/list-manga/'

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get links and names from the manga list of the current website.
function GetNameAndLink()
	local x = nil
	local u = MODULE.RootURL .. DirectoryPagination .. (URL + 1) .. '?order_by=name'

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	x.XPathHREFAll('//div[@class="mg_name"]/a', LINKS, NAMES)
	UPDATELIST.CurrentDirectoryPageNumber = tonumber(x.XPathString('//ul[@class="pagination"]/li[last()]/a/@data-ci-pagination-page')) or 1

	return no_error
end

-- Get info and chapter list for current manga.
function GetInfo()
	local v, x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	MANGAINFO.Title     = x.XPathString('//div[@class="detail_name"]/h1')
	MANGAINFO.CoverLink = x.XPathString('//div[@class="detail_avatar"]/img/@src')
	MANGAINFO.Authors   = x.XPathString('//div[@class="info_label" and contains(., "Author")]/following-sibling::div')
	MANGAINFO.Artists   = x.XPathString('//div[@class="info_label" and contains(., "Artist")]/following-sibling::div')
	MANGAINFO.Genres    = x.XPathStringAll('//div[@class="info_label" and contains(., "Categories")]/following-sibling::div/a')
	MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//div[@class="info_label" and contains(., "Status")]/following-sibling::div'), 'On Going', 'Success')
	MANGAINFO.Summary   = x.XPathString('//div[@class="detail_reviewContent"]')

	x.XPathHREFAll('//div[@class="chapter_box"]//a[not(@title)]', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()
	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	local v, x = nil
	local crypto = require 'fmd.crypto'
	local u = MaybeFillHost(MODULE.RootURL, URL)

	if not HTTP.GET(u) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	x.ParseHTML(GetBetween('var slides_p_path = ', ';', x.XPathString('//script[contains(., "slides_page")]')))
	for v in x.XPath('json(*)()').Get() do
		TASK.PageLinks.Add(crypto.DecodeBase64(v.ToString()))
	end

	return no_error
end